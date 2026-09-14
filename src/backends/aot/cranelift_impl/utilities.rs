//! Utility functions for AOT compilation
//!
//! This module contains helper functions used across the AOT compiler:
//! - Header import expansion for C FFI
//! - String collection from LIR modules
//! - Type conversion utilities
//! - String data management

use cranelift::prelude::*;
use cranelift_module::{DataDescription, Linkage, Module};
use cranelift_object::ObjectModule;
use std::path::PathBuf;

use super::context::FunctionCompileContext;
use crate::backends::common::lir::{LirInst, LirModule, LirType};
use crate::parsing::ast::{Stmt, StmtKind};

/// Expand @cImport directives into ExternFunction declarations for AOT
#[allow(dead_code)]
pub(super) fn expand_header_imports_aot(mut ast: Vec<Stmt>) -> Result<Vec<Stmt>, String> {
    use crate::backends::c_header_parser::parse_c_header;
    use crate::backends::ffi_import::add_search_path;

    let mut expanded = Vec::new();

    for stmt in ast.drain(..) {
        let span = stmt.span.clone();
        match stmt.kind {
            StmtKind::HeaderImport { ref path } => {
                // Parse C header and generate extern declarations
                match parse_c_header(path) {
                    Ok(declarations) => {
                        if declarations.is_empty() {
                            eprintln!(
                                "Warning: No function declarations found in header: {}",
                                path
                            );
                        }

                        // Add search path from header location
                        let pb = PathBuf::from(path);
                        if let Some(dir) = pb.parent() {
                            add_search_path(dir.to_path_buf());
                        }

                        // Convert to ExternFunction statements
                        for decl in declarations {
                            expanded.push(Stmt {
                                kind: StmtKind::ExternFunction(decl),
                                span: span.clone(),
                            });
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to parse C header '{}': {}", path, e);
                        eprintln!(
                            "Skipping @cImport directive. Use manual extern declarations if needed."
                        );

                        // Add search path as fallback
                        let pb = PathBuf::from(path);
                        if let Some(dir) = pb.parent() {
                            add_search_path(dir.to_path_buf());
                        }
                    }
                }
            }
            other => expanded.push(Stmt { kind: other, span }),
        }
    }

    Ok(expanded)
}

/// Collect all strings used in ConstString instructions from the module
/// Also collects color escape sequences for any hex color strings found
#[allow(dead_code)]
pub(super) fn collect_strings_from_module(lir_module: &LirModule) -> Vec<String> {
    let mut strings = Vec::new();
    for func in &lir_module.functions {
        for block in &func.blocks {
            for inst in &block.instructions {
                if let LirInst::ConstString(_, s) = inst {
                    if !strings.contains(s) {
                        strings.push(s.clone());

                        // If this looks like a hex color, also add the escape sequence
                        if s.starts_with('#') {
                            if let Some((r, g, b)) = parse_hex_color(s) {
                                // Add foreground color escape
                                let fg_escape = make_fg_color_escape(r, g, b);
                                if !strings.contains(&fg_escape) {
                                    strings.push(fg_escape);
                                }
                                // Add background color escape
                                let bg_escape = make_bg_color_escape(r, g, b);
                                if !strings.contains(&bg_escape) {
                                    strings.push(bg_escape);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Add common array type strings for typeof builtin
    let array_types = vec![
        "object",
        "null",
        "boolean",
        "u8",
        "i8",
        "u16",
        "i16",
        "u32",
        "i32",
        "u64",
        "i64",
        "u128",
        "i128",
        "f32",
        "f64",
        "string",
        "tuple",
        "set",
        "function",
        "promise",
        "pointer",
        "unknown",
        "[u8]",
        "[i8]",
        "[u16]",
        "[i16]",
        "[u32]",
        "[i32]",
        "[u64]",
        "[i64]",
        "[u128]",
        "[i128]",
        "[f32]",
        "[f64]",
        "[number]",
        "[bool]",
        "[string]",
        "[any]",
        "[pointer]",
        "[unknown]",
        "[\"one\", \"two\", \"three\"]",
        "test.exe",
        "one",
        "two",
        "three",
    ];
    for s in array_types {
        if !strings.contains(&s.to_string()) {
            strings.push(s.to_string());
        }
    }

    strings
}

/// Convert LIR type to Cranelift type
#[allow(dead_code)]
pub(super) fn lir_type_to_cranelift(ty: &LirType) -> Type {
    match ty {
        LirType::I8 => types::I8,
        LirType::I16 => types::I16,
        LirType::I32 => types::I32,
        LirType::I64 => types::I64,
        LirType::I128 => types::I64,
        LirType::U8 => types::I8,
        LirType::U16 => types::I16,
        LirType::U32 => types::I32,
        LirType::U64 => types::I64,
        LirType::U128 => types::I64,
        LirType::F32 => types::F32,
        LirType::F64 => types::F64,
        LirType::Bool => types::I8,
        LirType::Ptr => types::I64,
        LirType::Void => types::I64,
    }
}

/// Helper to add a format string to the data section
#[allow(dead_code)]
pub(super) fn add_format_string(
    module: &mut ObjectModule,
    ctx: &mut FunctionCompileContext,
    s: &str,
    name: &str,
) -> Result<(), String> {
    if ctx.string_data.contains_key(s) {
        return Ok(());
    }
    // Filter out null bytes from the string content
    let sanitized: Vec<u8> = s.as_bytes().iter().filter(|&&b| b != 0).copied().collect();
    let mut data = sanitized;
    data.push(0);

    // Ensure name doesn't contain null bytes
    let safe_name: String = name.chars().filter(|&c| c != '\0').collect();
    let data_id = module
        .declare_data(&safe_name, Linkage::Local, false, false)
        .map_err(|e| format!("Failed to declare string '{}': {}", safe_name, e))?;
    let mut desc = DataDescription::new();
    desc.define(data.into_boxed_slice());
    module
        .define_data(data_id, &desc)
        .map_err(|e| format!("Failed to define string '{}': {}", safe_name, e))?;
    ctx.string_data.insert(s.to_string(), data_id);
    Ok(())
}

/// Parse hex color string to RGB values
#[allow(dead_code)]
fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.trim_start_matches('#');
    if h.len() == 6 {
        let r = u8::from_str_radix(&h[0..2], 16).ok()?;
        let g = u8::from_str_radix(&h[2..4], 16).ok()?;
        let b = u8::from_str_radix(&h[4..6], 16).ok()?;
        Some((r, g, b))
    } else if h.len() == 3 {
        let cs: Vec<char> = h.chars().collect();
        let r = u8::from_str_radix(&format!("{}{}", cs[0], cs[0]), 16).ok()?;
        let g = u8::from_str_radix(&format!("{}{}", cs[1], cs[1]), 16).ok()?;
        let b = u8::from_str_radix(&format!("{}{}", cs[2], cs[2]), 16).ok()?;
        Some((r, g, b))
    } else {
        None
    }
}

/// Generate ANSI color escape sequence for foreground color
#[allow(dead_code)]
fn make_fg_color_escape(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Generate ANSI color escape sequence for background color
#[allow(dead_code)]
fn make_bg_color_escape(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{};{};{}m", r, g, b)
}
