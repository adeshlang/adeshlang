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
    /// - `-O[0-3]` / `--opt-level [0-3]`: Set optimization level
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
                "--opt-level" => {
                    let level_str = parser.next_arg()?;
                    config.opt_level = level_str
                        .parse::<u8>()
                        .map_err(|_| format!("Invalid optimization level: {}", level_str))?;
                    config.fast_compile = false;
                }

                // Output file
                "-o" | "--output" => {
                    config.output = Some(PathBuf::from(parser.next_arg()?));
                }

                // Debug info
                "-g" | "--debug" => {
                    config.debug_info = true;
                }

                // Emit type
                "--emit" => {
                    let emit_str = parser.next_arg()?;
                    config.emit = EmitType::from_str(&emit_str)
                        .ok_or_else(|| format!("Invalid emit type: {}", emit_str))?;
                }

                // Convenience emit types
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
}
