//! Build command argument parser
//!
//! This module provides utilities for parsing build-specific command-line arguments
//! and converting them into AotBuildConfig instances.

use crate::cli::build::{AotBuildConfig, EmitType};
use std::path::PathBuf;

/// Builder for parsing build command arguments
pub struct BuildArgParser {
    args: Vec<String>,
    index: usize,
}

impl BuildArgParser {
    /// Create a new parser for build arguments
    pub fn new(args: Vec<String>) -> Self {
        Self { args, index: 0 }
    }

    /// Parse arguments into an AotBuildConfig
    ///
    /// Supported flags:
    /// - `--fast` / `--dev`: Enable fast compilation mode
    /// - `-O[0-3]`, `-Os`, `-Oz` / `--opt-level`: Set optimization level
    /// - `--debug`: Include debug info
    /// - `-o` / `--output`: Specify output file
    /// - `--emit`: Specify output type (exe, lib, obj, shared, asm)
    /// - `--target`: Cross-compilation target triple
    /// - `-I` / `--include`: Add include directory
    /// - `-L` / `--lib-dir`: Add library search path
    /// - `-l` / `--link`: Add library to link
    /// - `--library-mode`: Skip main function (library mode)
    /// - `--library`: Alias for --emit=lib
    /// - `--shared`: Alias for --emit=shared
    /// - `--external-linker`: Delegate the final link to external LLVM
    ///   (clang + lld/lld-link) instead of the built-in adeshlink engine
    /// - `--run`: Run after building
    /// - `--verbose`: Verbose output
    /// - `--quiet`: Quiet mode
    /// - `--dry-run`: Show build plan without building
    /// - `--check`: Check only (don't emit)
    pub fn parse(args: Vec<String>) -> Result<AotBuildConfig, String> {
        if args.is_empty() {
            return Err("No input file specified".to_string());
        }

        let mut parser = Self::new(args);
        let mut config = AotBuildConfig::default();
        let mut input_set = false;

        while parser.index < parser.args.len() {
            let arg = &parser.args[parser.index].clone();
            parser.index += 1;

            match arg.as_str() {
                // Fast compilation mode
                "--fast" | "--dev" => {
                    config.fast_compile = true;
                    config.opt_level = 0; // No optimizations in fast mode
                }

                // Optimization level
                "-O0" => {
                    config.opt_level = 0;
                    config.fast_compile = false;
                }
                "-O1" => {
                    config.opt_level = 1;
                    config.fast_compile = false;
                }
                "-O2" => {
                    config.opt_level = 2;
                    config.fast_compile = false;
                }
                "-O3" => {
                    config.opt_level = 3;
                    config.fast_compile = false;
                }
                "-Os" => {
                    config.opt_level = 4;
                    config.fast_compile = false;
                }
                "-Oz" => {
                    config.opt_level = 5;
                    config.fast_compile = false;
                }
                "--opt-level" => {
                    let level_str = parser.next_arg()?;
                    config.opt_level = parse_opt_level(&level_str)?;
                    config.fast_compile = false;
                }
                arg if arg.starts_with("--opt-level=") => {
                    let level_str = &arg["--opt-level=".len()..];
                    config.opt_level = parse_opt_level(level_str)?;
                    config.fast_compile = false;
                }

                // Release mode shortcut (-O3, not fast)
                "--release" => {
                    config.opt_level = 3;
                    config.fast_compile = false;
                }

                // Profile and jobs
                "--profile" => {
                    config.verbose = true;
                }
                "--jobs" | "-j" => {
                    let _ = parser.next_arg();
                }
                arg if arg.starts_with("--jobs=") || arg.starts_with("-j=") => {}

                // Features
                "--features" => {
                    let _ = parser.next_arg();
                }
                arg if arg.starts_with("--features=") => {}

                // LTO, PGO, Sanitizer, Hardening
                "--lto" => config.lto = true,
                "--enable-lto" => {
                    config.lto = true;
                }
                "--no-lto" | "--disable-lto" => {
                    config.lto = false;
                }
                arg if arg.starts_with("--lto=") => {
                    let mode = &arg["--lto=".len()..];
                    config.lto = mode != "off" && mode != "no" && mode != "false";
                }

                // These used to be accepted and silently ignored, so a build
                // that asked for them got none of the requested behaviour.
                "--pgo" | "--sanitizer" | "--hardening" => {
                    return Err(format!("`{}` is not supported by `adesh build` yet", arg));
                }
                arg if arg.starts_with("--pgo=") => {
                    use adesh_codegen::opt::pgo::{PgoConfig, PgoMode};
                    let value = &arg["--pgo=".len()..];
                    let mut pgo = PgoConfig::default();
                    if value == "generate" {
                        pgo.mode = PgoMode::Generate;
                    } else if let Some(path) = value.strip_prefix("use=")
                        && !path.is_empty()
                    {
                        pgo.mode = PgoMode::Use;
                        pgo.profile_path = Some(PathBuf::from(path));
                    } else {
                        return Err(
                            "PGO mode not supported; use --pgo=generate or --pgo=use=<path>".into(),
                        );
                    }
                    config.pgo = Some(pgo);
                }
                arg if arg.starts_with("--sanitizer=") => {
                    let flag = arg.split('=').next().unwrap_or(arg);
                    return Err(format!("`{}` is not supported by `adesh build` yet", flag));
                }

                // Output file
                "-o" | "--output" => {
                    config.output = Some(PathBuf::from(parser.next_arg()?));
                }

                // Debug info
                "-g" | "--debug" => {
                    config.debug_info = true;
                }

                // Codegen backend
                "--codegen" | "--backend" => {
                    config.codegen_backend = parser.next_arg()?;
                }
                arg if arg.starts_with("--codegen=") => {
                    config.codegen_backend = arg["--codegen=".len()..].to_string();
                }

                // Deterministic build
                "--deterministic" => {
                    config.deterministic = true;
                }

                // Emit type
                "--emit" => {
                    let emit_str = parser.next_arg()?;
                    config.emit = EmitType::from_str(&emit_str)
                        .ok_or_else(|| format!("Invalid emit type: {}", emit_str))?;
                }
                arg if arg.starts_with("--emit=") => {
                    let emit_str = &arg["--emit=".len()..];
                    config.emit = EmitType::from_str(emit_str)
                        .ok_or_else(|| format!("Invalid emit type: {}", emit_str))?;
                }

                // Convenience emit types
                "--emit-adob" | "--adob" => {
                    config.emit = EmitType::Adob;
                }
                "--emit-object" | "--emit-obj" => {
                    config.emit = EmitType::Object;
                }
                "--library" | "--lib" => {
                    config.emit = EmitType::StaticLib;
                }
                "--shared" | "--dylib" => {
                    config.emit = EmitType::SharedLib;
                }

                // Target triple
                "--target" => {
                    config.target = Some(parser.next_arg()?);
                }
                arg if arg.starts_with("--target=") => {
                    config.target = Some(arg["--target=".len()..].to_string());
                }

                // Include directories
                "-I" | "--include" => {
                    config.include_dirs.push(PathBuf::from(parser.next_arg()?));
                }

                // Library directories
                "-L" | "--lib-dir" => {
                    config.lib_dirs.push(PathBuf::from(parser.next_arg()?));
                }

                // Libraries to link
                "-l" | "--link" => {
                    config.link_libs.push(parser.next_arg()?);
                }

                // Extra linker arguments
                "--linker-arg" => {
                    config.linker_args.push(parser.next_arg()?);
                }

                // External linker bridge: delegate the final link step to an
                // external LLVM toolchain (clang + lld/lld-link) instead of the
                // built-in adeshlink engine. Verify readiness with
                // `adesh gpu-check --external-linker`.
                "--external-linker" | "--external-toolchain" | "--use-llvm" => {
                    config.external_linker = true;
                }

                // Library mode
                "--library-mode" | "--lib-mode" => {
                    config.library_mode = true;
                }

                // Output modes
                "--verbose" => {
                    config.verbose = true;
                }
                "--quiet" => {
                    config.quiet = true;
                }
                "--dry-run" => {
                    config.dry_run = true;
                }
                "--check" => {
                    config.check_only = true;
                }
                "--run" => {
                    config.run_after_build = true;
                }

                // Positional arguments and input file
                _ if !arg.starts_with('-') => {
                    if !input_set {
                        config.input = PathBuf::from(arg);
                        input_set = true;
                    } else {
                        config.program_args.push(arg.clone());
                    }
                }

                _ => {
                    return Err(format!("Unknown argument: {}", arg));
                }
            }
        }

        if !input_set {
            return Err("No input file specified".to_string());
        }

        Ok(config)
    }

    /// Get the next argument
    fn next_arg(&mut self) -> Result<String, String> {
        if self.index < self.args.len() {
            let arg = self.args[self.index].clone();
            self.index += 1;
            Ok(arg)
        } else {
            Err("Expected argument but none found".to_string())
        }
    }
}

fn parse_opt_level(level: &str) -> Result<u8, String> {
    match level.to_ascii_lowercase().as_str() {
        "os" | "s" => Ok(4),
        "oz" | "z" => Ok(5),
        numeric => {
            let parsed = numeric
                .parse::<u8>()
                .map_err(|_| format!("Invalid optimization level: {}", level))?;
            if parsed <= 3 {
                Ok(parsed)
            } else {
                Err(format!("Invalid optimization level: {}", level))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_fast_mode() {
        let args = vec!["test.adesh".to_string(), "--fast".to_string()];
        let config = BuildArgParser::parse(args).unwrap();
        assert!(config.fast_compile);
        assert_eq!(config.opt_level, 0);
    }

    #[test]
    fn test_parse_optimization_levels() {
        for level in 0..=3 {
            let args = vec!["test.adesh".to_string(), format!("-O{}", level)];
            let config = BuildArgParser::parse(args).unwrap();
            assert_eq!(config.opt_level, level);
            assert!(!config.fast_compile);
        }
    }

    #[test]
    fn test_parse_size_optimization_levels() {
        for (flag, expected) in [("-Os", 4), ("-Oz", 5)] {
            let config =
                BuildArgParser::parse(vec!["test.adesh".to_string(), flag.to_string()]).unwrap();
            assert_eq!(config.opt_level, expected);
        }
        for (flag, expected) in [("s", 4), ("Os", 4), ("z", 5), ("Oz", 5)] {
            let config = BuildArgParser::parse(vec![
                "test.adesh".to_string(),
                "--opt-level".to_string(),
                flag.to_string(),
            ])
            .unwrap();
            assert_eq!(config.opt_level, expected);
        }
        assert!(
            BuildArgParser::parse(vec!["test.adesh".to_string(), "--opt-level=9".to_string(),])
                .is_err()
        );
    }

    #[test]
    fn test_unimplemented_flags_are_rejected() {
        for flag in [
            "--pgo",
            "--pgo=profile.data",
            "--sanitizer",
            "--sanitizer=address",
            "--hardening",
        ] {
            let args = vec!["test.adesh".to_string(), flag.to_string()];
            let err = BuildArgParser::parse(args).expect_err(flag);
            assert!(err.contains("not supported"), "{flag}: {err}");
        }
    }

    #[test]
    fn test_parse_debug_info() {
        let args = vec!["test.adesh".to_string(), "--debug".to_string()];
        let config = BuildArgParser::parse(args).unwrap();
        assert!(config.debug_info);
    }

    #[test]
    fn test_parse_output() {
        let args = vec![
            "test.adesh".to_string(),
            "-o".to_string(),
            "output.exe".to_string(),
        ];
        let config = BuildArgParser::parse(args).unwrap();
        assert_eq!(config.output, Some(PathBuf::from("output.exe")));
    }

    #[test]
    fn test_parse_emit_types() {
        let test_cases = vec![
            ("--library", EmitType::StaticLib),
            ("--shared", EmitType::SharedLib),
        ];

        for (flag, expected_emit) in test_cases {
            let args = vec!["test.adesh".to_string(), flag.to_string()];
            let config = BuildArgParser::parse(args).unwrap();
            assert_eq!(config.emit, expected_emit);
        }
    }

    #[test]
    fn test_parse_include_dirs() {
        let args = vec![
            "test.adesh".to_string(),
            "-I".to_string(),
            "/usr/include".to_string(),
            "-I".to_string(),
            "/usr/local/include".to_string(),
        ];
        let config = BuildArgParser::parse(args).unwrap();
        assert_eq!(config.include_dirs.len(), 2);
    }

    #[test]
    fn test_parse_libraries() {
        let args = vec![
            "test.adesh".to_string(),
            "-l".to_string(),
            "m".to_string(),
            "-l".to_string(),
            "pthread".to_string(),
        ];
        let config = BuildArgParser::parse(args).unwrap();
        assert_eq!(config.link_libs, vec!["m", "pthread"]);
    }

    #[test]
    fn test_parse_external_linker_flag() {
        let args = vec!["test.adesh".to_string(), "--external-linker".to_string()];
        let config = BuildArgParser::parse(args).unwrap();
        assert!(config.external_linker);
    }

    #[test]
    fn test_parse_external_linker_aliases() {
        for flag in ["--external-toolchain", "--use-llvm"] {
            let args = vec!["test.adesh".to_string(), flag.to_string()];
            let config = BuildArgParser::parse(args).unwrap();
            assert!(config.external_linker, "{flag} must set external_linker");
        }
    }

    #[test]
    fn test_external_linker_defaults_off() {
        let args = vec!["test.adesh".to_string()];
        let config = BuildArgParser::parse(args).unwrap();
        assert!(!config.external_linker);
    }
}
