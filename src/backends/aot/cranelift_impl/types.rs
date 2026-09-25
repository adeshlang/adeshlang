//! Type definitions for AOT compilation
//!
//! This module contains core type definitions used throughout the Cranelift AOT compiler,
//! including runtime type information, print options, and output formats.

use std::collections::HashMap;

/// Runtime type info for AOT values
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AotValueType {
    Int,
    Float,
    Bool,
    Char,
    String,
    Ptr,
    Unknown,
    // Fixed-width types
    U8,
    U16,
    U32,
    U64,
    U128,
    I8,
    I16,
    I32,
    I64,
    I128,
    F32,
    F64,
    // Array with element type and length
    Array(Box<AotValueType>, usize), // (element_type, length)
    // Raw array with element type and length (zero metadata overhead)
    RawArray(Box<AotValueType>, usize),
    // Set with element type and length
    Set(Box<AotValueType>, usize),
    // Tuple with element types
    Tuple(Vec<AotValueType>),
    // Handle to a runtime value (for JIT bridge)
    Handle,
}

impl AotValueType {
    #[allow(dead_code)]
    pub(super) fn element_type_name(&self) -> &'static str {
        match self {
            AotValueType::Set(..) => "set",
            AotValueType::Tuple(..) => "tuple",
            AotValueType::Array(..) | AotValueType::RawArray(..) => "array",
            AotValueType::U8 => "u8",
            AotValueType::I8 => "i8",
            AotValueType::U16 => "u16",
            AotValueType::I16 => "i16",
            AotValueType::U32 => "u32",
            AotValueType::I32 => "i32",
            AotValueType::U64 => "u64",
            AotValueType::I64 => "i64",
            AotValueType::U128 => "u128",
            AotValueType::I128 => "i128",
            AotValueType::F32 => "f32",
            AotValueType::F64 => "f64",
            AotValueType::Int | AotValueType::Float => "number",
            AotValueType::Bool => "bool",
            AotValueType::Char => "char",
            AotValueType::String => "string",
            _ => "number",
        }
    }

    #[allow(dead_code)]
    pub(super) fn metadata_size(&self) -> i64 {
        // Calculate metadata overhead for arrays based on element type
        // Elements of size <= 4 bytes use 16 bytes metadata (len: u32, cap: u32, ptr: 8)
        // Elements of size > 4 bytes use 24 bytes metadata (len: u64, cap: u64, ptr: 8)
        match self {
            AotValueType::U8
            | AotValueType::I8
            | AotValueType::Bool
            | AotValueType::U16
            | AotValueType::I16
            | AotValueType::U32
            | AotValueType::I32
            | AotValueType::F32
            | AotValueType::Char => 16,
            _ => 24,
        }
    }
}

/// Print options extracted from object literals at compile time
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub(super) struct PrintOptions {
    pub(super) sep: Option<String>,
    pub(super) end: Option<String>,
    pub(super) color: Option<String>,
    pub(super) background: Option<String>,
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) underline: bool,
    pub(super) strikethrough: bool,
    pub(super) pretty: Option<String>,
}

/// Parse a hex color string and return RGB components
#[allow(dead_code)]
pub(super) fn parse_hex_color(hex: &str) -> Option<(u8, u8, u8)> {
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
pub(super) fn make_fg_color_escape(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[38;2;{};{};{}m", r, g, b)
}

/// Generate ANSI color escape sequence for background color
#[allow(dead_code)]
pub(super) fn make_bg_color_escape(r: u8, g: u8, b: u8) -> String {
    format!("\x1b[48;2;{};{};{}m", r, g, b)
}

/// Output format for AOT compilation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputFormat {
    /// Native executable (.exe on Windows, no extension on Unix)
    Executable,
    /// Shared library (.dll on Windows, .so on Linux, .dylib on macOS)
    SharedLib,
    /// Static library (.lib on Windows, .a on Unix)
    StaticLib,
    /// Object file (.obj on Windows, .o on Unix)
    Object,
    /// Assembly file (.asm/.s)
    Assembly,
}

/// AOT compilation options
#[derive(Debug, Clone)]
pub struct AotOptions {
    /// Optimization level (0-3)
    pub opt_level: u8,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu")
    pub target_triple: Option<String>,
    /// Output format
    pub output_format: OutputFormat,
    /// Generate debug info
    pub debug_info: bool,
    /// Additional compiler flags
    pub flags: HashMap<String, String>,
    /// Library mode: when true, skip runtime initialization and CLI arg handling
    pub library_mode: bool,
    /// Additional include directories (-I)
    pub include_dirs: Vec<String>,
    /// Additional library search paths (-L)
    pub lib_dirs: Vec<String>,
    /// Libraries to link (-l)
    pub link_libs: Vec<String>,
    /// Extra linker arguments
    pub extra_linker_args: Vec<String>,
}

impl Default for AotOptions {
    fn default() -> Self {
        AotOptions {
            opt_level: 2,
            target_triple: None,
            output_format: OutputFormat::Executable,
            debug_info: false,
            flags: HashMap::new(),
            library_mode: false,
            include_dirs: Vec::new(),
            lib_dirs: Vec::new(),
            link_libs: Vec::new(),
            extra_linker_args: Vec::new(),
        }
    }
}
