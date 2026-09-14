//! Type definitions and helper functions for JIT execution

use crate::backends::jit::builtins::RuntimeValue;
use crate::parsing::ast::{Stmt, StmtKind};

/// Variable memory info for JIT detailed stats
#[derive(Debug, Clone)]
pub struct JitVariableInfo {
    pub name: String,
    pub type_name: String,
    pub size_bytes: usize,
}

/// Memory statistics for JIT execution
#[derive(Debug, Clone, Default)]
pub struct JitMemoryStats {
    /// Total bytes used by global variables
    pub total_variable_bytes: usize,
    /// Number of global variables
    pub variable_count: usize,
    /// Detailed info per variable
    pub variables: Vec<JitVariableInfo>,
    /// Memory by type category
    pub by_type: std::collections::HashMap<String, (usize, usize)>, // type -> (count, bytes)
    /// Number of compiled functions
    pub function_count: usize,
    /// Number of active promises
    pub promise_count: usize,
    /// Memoization cache entries
    pub memo_cache_entries: usize,
}

/// Expand @cImport directives into ExternFunction declarations
pub(crate) fn expand_header_imports(mut ast: Vec<Stmt>) -> Result<Vec<Stmt>, String> {
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

/// Helper to get type name for RuntimeValue
pub(crate) fn runtime_type_name(value: &RuntimeValue) -> String {
    match value {
        RuntimeValue::Int(n) => {
            let t = if *n >= 0 {
                if *n <= u8::MAX as i64 {
                    "u8"
                } else if *n <= u16::MAX as i64 {
                    "u16"
                } else if *n <= u32::MAX as i64 {
                    "u32"
                } else {
                    "u64"
                }
            } else if *n >= i8::MIN as i64 && *n <= i8::MAX as i64 {
                "i8"
            } else if *n >= i16::MIN as i64 && *n <= i16::MAX as i64 {
                "i16"
            } else if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                "i32"
            } else {
                "i64"
            };
            t.to_string()
        }
        RuntimeValue::Float(n) => {
            let t = if n.fract().abs() < 1e-12 {
                let i = *n as i128;
                if i >= 0 {
                    if i <= u8::MAX as i128 {
                        "u8"
                    } else if i <= u16::MAX as i128 {
                        "u16"
                    } else if i <= u32::MAX as i128 {
                        "u32"
                    } else if i <= u64::MAX as i128 {
                        "u64"
                    } else {
                        "u128"
                    }
                } else if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                    "i8"
                } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                    "i16"
                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                    "i32"
                } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                    "i64"
                } else {
                    "i128"
                }
            } else {
                "f64"
            };
            t.to_string()
        }
        RuntimeValue::Bool(_) => "bool".to_string(),
        RuntimeValue::Char(_) => "char".to_string(),
        RuntimeValue::String(_) => "string".to_string(),
        RuntimeValue::Array(_) => "array".to_string(),
        RuntimeValue::Set(_) => "set".to_string(),
        RuntimeValue::Tuple(_) => "tuple".to_string(),
        RuntimeValue::Object(_) => "object".to_string(),
        RuntimeValue::Promise(_) => "promise".to_string(),
        RuntimeValue::Function(_) => "function".to_string(),
        RuntimeValue::BigInt(_) => "bigint".to_string(),
        RuntimeValue::Null => "null".to_string(),
        // Fixed-width integer types (unsigned)
        RuntimeValue::U8(_) => "u8".to_string(),
        RuntimeValue::U16(_) => "u16".to_string(),
        RuntimeValue::U32(_) => "u32".to_string(),
        RuntimeValue::U64(_) => "u64".to_string(),
        RuntimeValue::U128(_) => "u128".to_string(),
        // Fixed-width integer types (signed)
        RuntimeValue::I8(_) => "i8".to_string(),
        RuntimeValue::I16(_) => "i16".to_string(),
        RuntimeValue::I32(_) => "i32".to_string(),
        RuntimeValue::I64(_) => "i64".to_string(),
        RuntimeValue::I128(_) => "i128".to_string(),
        // Fixed-width float types
        RuntimeValue::F32(_) => "f32".to_string(),
        RuntimeValue::F64(_) => "f64".to_string(),
        RuntimeValue::RawArray(elem_type, _) => format!("[{};raw]", elem_type),
        RuntimeValue::DynArray { concrete_type, .. } => format!("[{}]", concrete_type),
    }
}

/// Helper to calculate size of RuntimeValue
pub(crate) fn sizeof_runtime_value(value: &RuntimeValue) -> usize {
    match value {
        RuntimeValue::Int(n) => {
            if *n >= 0 {
                if *n <= u8::MAX as i64 {
                    1
                } else if *n <= u16::MAX as i64 {
                    2
                } else if *n <= u32::MAX as i64 {
                    4
                } else {
                    8
                }
            } else if *n >= i8::MIN as i64 && *n <= i8::MAX as i64 {
                1
            } else if *n >= i16::MIN as i64 && *n <= i16::MAX as i64 {
                2
            } else if *n >= i32::MIN as i64 && *n <= i32::MAX as i64 {
                4
            } else {
                8
            }
        }
        RuntimeValue::Float(n) => {
            if n.fract().abs() < 1e-12 {
                let i = *n as i128;
                if i >= 0 {
                    if i <= u8::MAX as i128 {
                        1
                    } else if i <= u16::MAX as i128 {
                        2
                    } else if i <= u32::MAX as i128 {
                        4
                    } else if i <= u64::MAX as i128 {
                        8
                    } else {
                        16
                    }
                } else if i >= i8::MIN as i128 && i <= i8::MAX as i128 {
                    1
                } else if i >= i16::MIN as i128 && i <= i16::MAX as i128 {
                    2
                } else if i >= i32::MIN as i128 && i <= i32::MAX as i128 {
                    4
                } else if i >= i64::MIN as i128 && i <= i64::MAX as i128 {
                    8
                } else {
                    16
                }
            } else {
                8
            }
        }
        RuntimeValue::Bool(_) => 1,
        RuntimeValue::Char(_) => 4,
        RuntimeValue::Null => 0,
        RuntimeValue::String(s) => s.len(),
        RuntimeValue::BigInt(bi) => {
            // BigInt size varies based on magnitude
            8 + (bi.bits() as usize / 32 + 1) * 4
        }
        RuntimeValue::Array(arr) => {
            let base = 24;
            let elements: usize = arr.iter().map(sizeof_runtime_value).sum();
            base + elements
        }
        RuntimeValue::Set(set_vals) => {
            let base = 24;
            let elements: usize = set_vals.iter().map(sizeof_runtime_value).sum();
            base + elements
        }
        RuntimeValue::Tuple(tup) => {
            let base = 24;
            let elements: usize = tup.iter().map(sizeof_runtime_value).sum();
            base + elements
        }
        RuntimeValue::RawArray(_, arr) => {
            // Raw arrays have zero overhead
            arr.iter().map(sizeof_runtime_value).sum()
        }
        RuntimeValue::DynArray {
            data,
            element_type,
            concrete_type,
            ..
        } => {
            // Metadata size depends on element type
            let metadata = if element_type.starts_with("u8")
                || element_type.starts_with("i8")
                || element_type.starts_with("u16")
                || element_type.starts_with("i16")
                || element_type.starts_with("u32")
                || element_type.starts_with("i32")
                || element_type.starts_with("f32")
            {
                16 // Byte/Short/Word: 16-byte metadata
            } else {
                24 // Long/Extended/Any: 24-byte metadata
            };
            let elements: usize = data.iter().map(sizeof_runtime_value).sum();
            metadata + concrete_type.len() + elements
        }
        RuntimeValue::Object(obj) => {
            let base = 48;
            let contents: usize = obj
                .iter()
                .map(|(k, v)| k.len() + sizeof_runtime_value(v))
                .sum();
            base + contents
        }
        RuntimeValue::Promise(_) => 8,
        RuntimeValue::Function(f) => {
            let name_size = f.name.len();
            let params_size: usize = f.params.iter().map(|p| p.len()).sum();
            let captures_size: usize = f
                .captures
                .iter()
                .map(|(k, v)| k.len() + sizeof_runtime_value(v))
                .sum();
            24 + name_size + params_size + captures_size
        }
        // Fixed-width integer types (unsigned)
        RuntimeValue::U8(_) => 1,
        RuntimeValue::U16(_) => 2,
        RuntimeValue::U32(_) => 4,
        RuntimeValue::U64(_) => 8,
        RuntimeValue::U128(_) => 16,
        // Fixed-width integer types (signed)
        RuntimeValue::I8(_) => 1,
        RuntimeValue::I16(_) => 2,
        RuntimeValue::I32(_) => 4,
        RuntimeValue::I64(_) => 8,
        RuntimeValue::I128(_) => 16,
        // Fixed-width float types
        RuntimeValue::F32(_) => 4,
        RuntimeValue::F64(_) => 8,
    }
}
