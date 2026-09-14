//! Utility functions and types for Cranelift AOT backend
//!
//! This module contains helper functions and type definitions used throughout
//! the Cranelift AOT compiler.

use crate::parsing::ast::{Stmt, StmtKind};

/// Expand @cImport directives into ExternFunction declarations for AOT
pub(crate) fn expand_header_imports_aot(mut ast: Vec<Stmt>) -> Result<Vec<Stmt>, String> {
    use crate::backends::c_header_parser::parse_c_header;
    use crate::backends::ffi_import::add_search_path;
    use std::path::PathBuf;

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


/// Parse a hex color string and return RGB components
pub(crate) fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
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
pub(crate) fn make_fg_color_escape(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Generate ANSI color escape sequence for background color
pub(crate) fn make_bg_color_escape(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{};{};{}m", r, g, b)
}
