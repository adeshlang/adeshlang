//! Public API for running bytecode files.
//!
//! Provides high-level functions to execute language bytecode:
//! - `run_file`: Execute bytecode and print to stdout
//! - `run_file_with_writer`: Execute bytecode with custom output writer
//! - `run_file_with_args`: Execute bytecode with program arguments
//! - `run_file_with_writer_and_args`: Execute bytecode with custom output and arguments

use std::io::Write;
use std::path::Path;

use super::v1_stack;
use super::v2_register;

/// Bytecode magics for v1 (stack) and v2 (register)
/// NOTE: These must match the MAGIC constants in bytecode.rs
const MAGIC_V1: &[u8] = b"Adesh-BC\n";
const MAGIC_V2: &[u8] = b"Adesh-BC2\n";

/// Run the bytecode located at `path`, writing any VM output to the provided
/// writer. This is useful for tests to capture stdout programmatically.
#[allow(dead_code)]
pub fn run_file_with_writer(path: &Path, out: &mut dyn Write) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.starts_with(MAGIC_V1) {
        // v1 stack VM path
        let idx = MAGIC_V1.len();
        v1_stack::execute_v1(&data, idx, out)
    } else if data.starts_with(MAGIC_V2) {
        // v2 register VM path
        let idx = MAGIC_V2.len();
        v2_register::execute_v2(&data, idx, out)
    } else {
        Err("not a supported bytecode file".into())
    }
}

/// Convenience wrapper that writes output to stdout.
#[allow(dead_code)]
pub fn run_file(path: &Path) -> Result<(), String> {
    let mut out_buf: Vec<u8> = Vec::new();
    run_file_with_writer(path, &mut out_buf)?;
    let s = String::from_utf8_lossy(&out_buf).to_string();
    println!("{}", s);
    Ok(())
}

/// Convenience wrapper that runs bytecode with program arguments.
#[allow(dead_code)]
pub fn run_file_with_args(path: &Path, args: &[String]) -> Result<(), String> {
    let mut out_buf: Vec<u8> = Vec::new();
    run_file_with_writer_and_args(path, &mut out_buf, args)?;
    let s = String::from_utf8_lossy(&out_buf).to_string();
    println!("{}", s);
    Ok(())
}

/// Run the bytecode located at `path` with program arguments, writing any VM output to the provided writer.
#[allow(dead_code)]
pub fn run_file_with_writer_and_args(
    path: &Path,
    out: &mut dyn Write,
    args: &[String],
) -> Result<(), String> {
    let data = std::fs::read(path).map_err(|e| e.to_string())?;
    if data.starts_with(MAGIC_V1) {
        // v1 stack VM path - arguments not supported in v1
        return run_file_with_writer(path, out);
    } else if data.starts_with(MAGIC_V2) {
        // v2 register VM path with arguments
        let idx = MAGIC_V2.len();
        let path_str = path.to_string_lossy().to_string();
        v2_register::execute_v2_with_args(&data, idx, out, args, path_str)
    } else {
        Err("not a supported bytecode file".into())
    }
}
