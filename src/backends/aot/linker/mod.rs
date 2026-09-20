/// Platform-specific linker driver for AOT compilation
///
/// This module provides abstraction over platform-specific linking rules
/// for generating executables, shared libraries, static libraries, and object files
/// with correct startup objects, entry points, and ABI conventions.
use std::path::{Path, PathBuf};
use std::process::Command;
use target_lexicon::Triple;

/// Output type for linking
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputType {
    /// Native executable (.exe on Windows, no extension on Unix)
    Executable,
    /// Shared library (.dll on Windows, .so on Linux, .dylib on macOS)
    SharedLib,
    /// Static library (.lib on Windows, .a on Unix)
    StaticLib,
}

/// Linker configuration for AOT compilation
#[derive(Debug, Clone)]
pub struct LinkerConfig {
    pub output_type: OutputType,
    pub target_triple: Triple,
    pub opt_level: u8,
    pub debug_info: bool,
    pub include_dirs: Vec<PathBuf>,
    pub lib_dirs: Vec<PathBuf>,
    pub link_libs: Vec<String>,
    pub extra_linker_args: Vec<String>,
    pub library_mode: bool,
}

impl Default for LinkerConfig {
    fn default() -> Self {
        LinkerConfig {
            output_type: OutputType::Executable,
            target_triple: Triple::host(),
            opt_level: 2,
            debug_info: false,
            include_dirs: Vec::new(),
            lib_dirs: Vec::new(),
            link_libs: Vec::new(),
            extra_linker_args: Vec::new(),
            library_mode: false,
        }
    }
}

/// Platform-specific linker driver
pub struct LinkerDriver {
    config: LinkerConfig,
}

impl LinkerDriver {
    /// Create a new linker driver with the given configuration
    pub fn new(config: LinkerConfig) -> Self {
        LinkerDriver { config }
    }

    /// Link object files to produce final output
    ///
    /// # Arguments
    /// * `obj_path` - Path to the Cranelift-generated object file
    /// * `runtime_obj` - Path to the Adesh runtime object file
    /// * `output` - Output file path
    ///
    /// # Returns
    /// Ok(()) on successful linking, Err(String) with error message otherwise
    pub fn link(&self, obj_path: &Path, runtime_obj: &Path, output: &Path) -> Result<(), String> {
        let host_triple = Triple::host();

        // If target matches host, use native linker
        if self.config.target_triple == host_triple {
            return self.link_native(obj_path, runtime_obj, output);
        }

        // Cross-compilation: use cross-compiler tools
        self.link_cross(obj_path, runtime_obj, output)
    }

    /// Link using native linker (target matches host)
    fn link_native(
        &self,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        match self.config.target_triple.operating_system {
            target_lexicon::OperatingSystem::Windows => {
                self.link_windows(obj_path, runtime_obj, output)
            }
            target_lexicon::OperatingSystem::Linux => {
                self.link_linux(obj_path, runtime_obj, output)
            }
            target_lexicon::OperatingSystem::MacOSX { .. }
            | target_lexicon::OperatingSystem::Darwin => {
                self.link_macos(obj_path, runtime_obj, output)
            }
            _ => self.link_generic(obj_path, runtime_obj, output),
        }
    }

    /// Link using cross-compiler tools
    fn link_cross(&self, obj_path: &Path, runtime_obj: &Path, output: &Path) -> Result<(), String> {
        let target_prefix = self.get_target_prefix(&self.config.target_triple);

        // Build a list of potential cross-compilers to try
        let mut compilers_to_try: Vec<(String, Vec<String>)> = Vec::new();

        // Add target-prefixed GCC if we have a prefix
        if let Ok(prefix) = &target_prefix {
            compilers_to_try.push((
                format!("{}-gcc", prefix),
                self.get_linker_args(&self.config.target_triple, obj_path, runtime_obj, output),
            ));
        }

        // Add clang with --target flag
        let mut clang_args = vec![format!("--target={}", self.config.target_triple)];
        clang_args.extend(self.get_linker_args(
            &self.config.target_triple,
            obj_path,
            runtime_obj,
            output,
        ));
        compilers_to_try.push(("clang".to_string(), clang_args));

        // Try each compiler
        for (compiler, args) in compilers_to_try {
            if self.try_compiler(&compiler, &args).is_ok() {
                return Ok(());
            }
        }

        Err(format!(
            "Cross-compilation linking failed for target {}. \
             Install cross-compiler (e.g., {}gcc) or use --object to generate object file only.",
            self.config.target_triple,
            target_prefix
                .as_ref()
                .map(|p| format!("{}-", p))
                .unwrap_or_default()
        ))
    }

    /// Windows linking with proper DLL/EXE rules
    fn link_windows(
        &self,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        // Try MSVC linker first
        if self.try_msvc_link(obj_path, runtime_obj, output).is_ok() {
            return Ok(());
        }

        // Fall back to GCC/Clang
        self.try_gcc_clang_link(obj_path, runtime_obj, output)
    }

    /// Link with MSVC linker (Windows)
    fn try_msvc_link(
        &self,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        let mut cmd = Command::new("link.exe");
        cmd.arg("/NOLOGO");

        // Set entry point and subsystem based on output type
        match self.config.output_type {
            OutputType::Executable => {
                cmd.arg("/ENTRY:main");
                cmd.arg("/SUBSYSTEM:CONSOLE");
            }
            OutputType::SharedLib => {
                cmd.arg("/DLL");
                // For DLL, we don't set entry point
                // Generate import library with same base name
                let import_lib = output.with_extension("lib");
                cmd.arg(format!("/IMPLIB:{}", import_lib.display()));
            }
            OutputType::StaticLib => {
                // Static library uses lib.exe, not link.exe
                return Err("Use lib.exe for static libraries".to_string());
            }
        }

        cmd.arg(format!("/OUT:{}", output.display()));
        cmd.arg(obj_path);

        // Only add runtime if not in library mode
        if !self.config.library_mode {
            cmd.arg(runtime_obj);
        }

        // Add Windows libraries only for executable
        if matches!(self.config.output_type, OutputType::Executable) {
            cmd.arg("kernel32.lib");
            cmd.arg("msvcrt.lib");
        }

        // Add user libraries
        for lib in &self.config.link_libs {
            cmd.arg(format!("{}.lib", lib));
        }

        let output_result = cmd
            .output()
            .map_err(|e| format!("Failed to execute link.exe: {}", e))?;

        if output_result.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            Err(format!("MSVC linking failed: {}", stderr))
        }
    }

    /// Linux linking with proper shared library rules
    fn link_linux(&self, obj_path: &Path, runtime_obj: &Path, output: &Path) -> Result<(), String> {
        // Try ld first, then fall back to GCC/Clang
        if self.try_ld_link(obj_path, runtime_obj, output).is_ok() {
            return Ok(());
        }

        self.try_gcc_clang_link(obj_path, runtime_obj, output)
    }

    /// Link with ld (Linux)
    fn try_ld_link(
        &self,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        let mut cmd = Command::new("ld");
        cmd.arg("-o");
        cmd.arg(output);
        cmd.arg(obj_path);
        cmd.arg(runtime_obj);

        match self.config.output_type {
            OutputType::Executable => {
                cmd.arg("-lc");
                // Add dynamic linker
                if let Ok(linker) = self.get_linux_dynamic_linker() {
                    cmd.arg("-dynamic-linker");
                    cmd.arg(linker);
                }
            }
            OutputType::SharedLib => {
                cmd.arg("-shared");
                cmd.arg("-lc");
                // Add SONAME for shared libraries
                if let Some(filename) = output.file_name() {
                    cmd.arg("-soname");
                    cmd.arg(filename);
                }
            }
            OutputType::StaticLib => {
                // ar, not ld, is used for static libraries
                return Err("Use ar for static libraries".to_string());
            }
        }

        let output_result = cmd
            .output()
            .map_err(|e| format!("Failed to execute ld: {}", e))?;

        if output_result.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output_result.stderr);
            Err(format!("ld linking failed: {}", stderr))
        }
    }

    /// macOS linking with proper shared library rules
    fn link_macos(&self, obj_path: &Path, runtime_obj: &Path, output: &Path) -> Result<(), String> {
        // Try ld first, then fall back to clang
        if self.try_ld_link(obj_path, runtime_obj, output).is_ok() {
            return Ok(());
        }

        self.try_gcc_clang_link(obj_path, runtime_obj, output)
    }

    /// Generic linking (fallback)
    fn link_generic(
        &self,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        self.try_gcc_clang_link(obj_path, runtime_obj, output)
    }

    /// Link with GCC or Clang
    fn try_gcc_clang_link(
        &self,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        // Try gcc first, then clang
        let compilers = match self.config.target_triple.operating_system {
            target_lexicon::OperatingSystem::MacOSX { .. }
            | target_lexicon::OperatingSystem::Darwin => {
                vec!["clang", "gcc"]
            }
            _ => vec!["gcc", "clang"],
        };

        let mut last_error: Option<String> = None;

        for compiler in compilers {
            match self.try_gcc_clang_compiler(compiler, obj_path, runtime_obj, output) {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_error = Some(err);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| "Neither gcc nor clang available for linking".to_string()))
    }

    /// Try specific GCC/Clang compiler
    fn try_gcc_clang_compiler(
        &self,
        compiler: &str,
        obj_path: &Path,
        runtime_obj: &Path,
        output: &Path,
    ) -> Result<(), String> {
        // For Windows, try direct linking without special target flags
        // to avoid issues with gold plugin loader paths
        let is_windows =
            self.config.target_triple.operating_system == target_lexicon::OperatingSystem::Windows;

        // Try different linker configurations if the default fails
        let linker_configs = if is_windows {
            // For Windows, prioritize lld which doesn't use GNU plugin system
            vec![
                vec!["-fuse-ld=lld"], // Use LLVM's lld linker (no plugins)
                vec!["-fuse-ld=bfd"], // Try BFD
                vec![],               // Try defaults
            ]
        } else {
            // For non-Windows, different order
            vec![
                vec!["-fuse-ld=lld"], // Try LLD
                vec!["-fuse-ld=bfd"], // Try BFD
                vec![],               // Try with defaults
            ]
        };

        let mut last_error = String::new();

        for linker_flags in linker_configs {
            let mut cmd = Command::new(compiler);
            cmd.arg("-o");
            cmd.arg(output);
            cmd.arg(obj_path);

            // Only add runtime if not in library mode
            if !self.config.library_mode {
                cmd.arg(runtime_obj);
            }

            // Add linker configuration flags
            for flag in &linker_flags {
                cmd.arg(flag);
            }

            // Add optimization flags
            match self.config.opt_level {
                0 => {
                    cmd.arg("-O0");
                }
                1 => {
                    cmd.arg("-O1");
                }
                2 => {
                    cmd.arg("-O2");
                    // Add aggressive optimization flags for size reduction
                    cmd.arg("-ffunction-sections"); // Separate function sections
                    cmd.arg("-fdata-sections"); // Separate data sections
                    match self.config.target_triple.operating_system {
                        target_lexicon::OperatingSystem::MacOSX { .. }
                        | target_lexicon::OperatingSystem::Darwin => {
                            cmd.arg("-Wl,-dead_strip"); // macOS
                        }
                        _ => {
                            cmd.arg("-Wl,--gc-sections"); // GNU ld/ld.lld
                        }
                    }
                }
                3 => {
                    cmd.arg("-O3");
                    // Maximum optimization
                    cmd.arg("-flto"); // Enable LTO
                    cmd.arg("-ffunction-sections");
                    cmd.arg("-fdata-sections");
                    match self.config.target_triple.operating_system {
                        target_lexicon::OperatingSystem::MacOSX { .. }
                        | target_lexicon::OperatingSystem::Darwin => {
                            cmd.arg("-Wl,-dead_strip");
                        }
                        _ => {
                            cmd.arg("-Wl,--gc-sections");
                        }
                    }
                }
                _ => {
                    cmd.arg("-O2");
                    cmd.arg("-ffunction-sections");
                    cmd.arg("-fdata-sections");
                    match self.config.target_triple.operating_system {
                        target_lexicon::OperatingSystem::MacOSX { .. }
                        | target_lexicon::OperatingSystem::Darwin => {
                            cmd.arg("-Wl,-dead_strip");
                        }
                        _ => {
                            cmd.arg("-Wl,--gc-sections");
                        }
                    }
                }
            };

            // Add debug info if requested
            if self.config.debug_info {
                cmd.arg("-g");
            } else {
                // Strip symbols and debug info for smaller output
                cmd.arg("-s");
            }

            // Add PIC for shared libraries
            if self.config.output_type == OutputType::SharedLib {
                cmd.arg("-shared");

                match self.config.target_triple.operating_system {
                    target_lexicon::OperatingSystem::Windows => {
                        // Windows DLL specific flags
                        cmd.arg("-Wl,--out-implib");
                        let implib = output.with_extension("lib");
                        cmd.arg(format!("-Wl,{}", implib.display()));
                        cmd.arg("-Wl,--export-all-symbols");
                        cmd.arg("-Wl,--enable-auto-image-base");
                    }
                    target_lexicon::OperatingSystem::Linux => {
                        // Linux SO specific flags
                        cmd.arg("-fPIC");
                        if let Some(filename) = output.file_name() {
                            cmd.arg("-Wl,-soname");
                            cmd.arg(format!("-Wl,{}", filename.to_string_lossy()));
                        }
                    }
                    target_lexicon::OperatingSystem::MacOSX { .. }
                    | target_lexicon::OperatingSystem::Darwin => {
                        // macOS dylib specific flags
                        cmd.arg("-fPIC");
                    }
                    _ => {
                        cmd.arg("-fPIC");
                    }
                }
            }

            // Add include directories
            for dir in &self.config.include_dirs {
                cmd.arg(format!("-I{}", dir.display()));
            }

            // Add library directories
            for dir in &self.config.lib_dirs {
                cmd.arg(format!("-L{}", dir.display()));
            }

            // Add libraries
            for lib in &self.config.link_libs {
                cmd.arg(format!("-l{}", lib));
            }

            // Add platform-specific libraries only for executables
            if !self.config.library_mode {
                match self.config.target_triple.operating_system {
                    target_lexicon::OperatingSystem::Linux => {
                        cmd.arg("-no-pie");
                        cmd.arg("-lc").arg("-lpthread").arg("-ldl").arg("-lm");
                    }
                    target_lexicon::OperatingSystem::MacOSX { .. }
                    | target_lexicon::OperatingSystem::Darwin => {
                        cmd.arg("-lSystem");
                    }
                    target_lexicon::OperatingSystem::Windows => {
                        cmd.arg("-lkernel32");
                        cmd.arg("-lmsvcrt");
                    }
                    _ => {}
                }
            }

            // Add extra linker arguments
            for arg in &self.config.extra_linker_args {
                cmd.arg(arg);
            }

            let output_result = cmd
                .output()
                .map_err(|e| format!("Failed to execute {}: {}", compiler, e))?;

            if output_result.status.success() {
                return Ok(());
            } else {
                let stderr = String::from_utf8_lossy(&output_result.stderr);
                last_error = format!(
                    "{} linking failed: {} with flags {:?}",
                    compiler, stderr, linker_flags
                );
                // Continue trying with next linker config
            }
        }

        Err(last_error)
    }

    /// Try to execute a compiler with given arguments
    fn try_compiler(&self, compiler: &str, args: &[String]) -> Result<(), String> {
        let result = Command::new(compiler).args(args).output();

        match result {
            Ok(output) => {
                if output.status.success() {
                    Ok(())
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    Err(format!("{}: {}", compiler, stderr))
                }
            }
            Err(e) => Err(format!("{} not found: {}", compiler, e)),
        }
    }

    /// Get linker arguments for the target
    fn get_linker_args(
        &self,
        _target: &Triple,
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

        // Add optimization flags
        match self.config.opt_level {
            0 => args.push("-O0".to_string()),
            1 => args.push("-O1".to_string()),
            2 => args.push("-O2".to_string()),
            3 => args.push("-O3".to_string()),
            _ => args.push("-O2".to_string()),
        }

        // Add library search paths and libraries
        for dir in &self.config.lib_dirs {
            args.push(format!("-L{}", dir.display()));
        }

        for lib in &self.config.link_libs {
            args.push(format!("-l{}", lib));
        }

        args
    }

    /// Get the Linux dynamic linker path
    fn get_linux_dynamic_linker(&self) -> Result<String, String> {
        match self.config.target_triple.architecture {
            target_lexicon::Architecture::X86_64 => Ok("/lib64/ld-linux-x86-64.so.2".to_string()),
            target_lexicon::Architecture::Aarch64(_) => {
                Ok("/lib/ld-linux-aarch64.so.1".to_string())
            }
            target_lexicon::Architecture::Arm(_) => Ok("/lib/ld-linux-armhf.so.3".to_string()),
            target_lexicon::Architecture::X86_32(_) => Ok("/lib/ld-linux.so.2".to_string()),
            _ => Ok("/lib64/ld-linux-x86-64.so.2".to_string()),
        }
    }

    /// Get target prefix for cross-compiler
    fn get_target_prefix(&self, target: &Triple) -> Result<String, String> {
        let arch = match target.architecture {
            target_lexicon::Architecture::X86_64 => "x86_64",
            target_lexicon::Architecture::X86_32(_) => "i686",
            target_lexicon::Architecture::Aarch64(_) => "aarch64",
            target_lexicon::Architecture::Arm(_) => "arm",
            _ => return Err("Unknown architecture".to_string()),
        };

        let os_env = match target.operating_system {
            target_lexicon::OperatingSystem::Linux => match target.environment {
                target_lexicon::Environment::Gnu => "linux-gnu",
                target_lexicon::Environment::Musl => "linux-musl",
                _ => "linux-gnu",
            },
            target_lexicon::OperatingSystem::Windows => "w64-mingw32",
            _ => return Err("Unsupported OS for cross-compilation".to_string()),
        };

        Ok(format!("{}-{}", arch, os_env))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linker_config_default() {
        let config = LinkerConfig::default();
        assert_eq!(config.output_type, OutputType::Executable);
        assert_eq!(config.opt_level, 2);
        assert!(!config.debug_info);
    }

    #[test]
    fn test_linker_driver_creation() {
        let config = LinkerConfig::default();
        let driver = LinkerDriver::new(config);
        assert_eq!(driver.config.output_type, OutputType::Executable);
    }
}
