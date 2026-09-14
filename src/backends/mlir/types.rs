//! MLIR Type Conversions
//!
//! Utilities for converting between VIR types and MLIR types.

use crate::ir::vir::VirType;

/// Convert VIR type to MLIR type string
pub fn vir_to_mlir_type(ty: &VirType) -> String {
    match ty {
        VirType::I8 => "i8".to_string(),
        VirType::I16 => "i16".to_string(),
        VirType::I32 => "i32".to_string(),
        VirType::I64 => "i64".to_string(),
        VirType::U8 => "i8".to_string(),
        VirType::U16 => "i16".to_string(),
        VirType::U32 => "i32".to_string(),
        VirType::U64 => "i64".to_string(),
        VirType::F32 => "f32".to_string(),
        VirType::F64 => "f64".to_string(),
        VirType::Bool => "i1".to_string(),
        VirType::Ptr => "!llvm.ptr".to_string(),
        VirType::Void => "()".to_string(),
        _ => "!llvm.ptr".to_string(), // Default to pointer for complex types
    }
}

/// Get MLIR integer type for size
pub fn mlir_int_type(bits: u32) -> String {
    format!("i{}", bits)
}

/// Get MLIR float type for size
pub fn mlir_float_type(bits: u32) -> String {
    format!("f{}", bits)
}

/// Get MLIR memref type
pub fn mlir_memref_type(element_ty: &str, shape: Option<&[i64]>) -> String {
    if let Some(dims) = shape {
        let shape_str = dims
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join("x");
        format!("memref<{}x{}>", shape_str, element_ty)
    } else {
        format!("memref<?x{}>", element_ty)
    }
}

/// Get MLIR function type
pub fn mlir_function_type(params: &[String], returns: &[String]) -> String {
    let params_str = params.join(", ");
    let returns_str = if returns.is_empty() {
        "()".to_string()
    } else if returns.len() == 1 {
        returns[0].clone()
    } else {
        format!("({})", returns.join(", "))
    };

    format!("({}) -> {}", params_str, returns_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vir_to_mlir_type() {
        assert_eq!(vir_to_mlir_type(&VirType::I64), "i64");
        assert_eq!(vir_to_mlir_type(&VirType::F32), "f32");
        assert_eq!(vir_to_mlir_type(&VirType::Bool), "i1");
        assert_eq!(vir_to_mlir_type(&VirType::Ptr), "!llvm.ptr");
    }

    #[test]
    fn test_mlir_int_type() {
        assert_eq!(mlir_int_type(32), "i32");
        assert_eq!(mlir_int_type(64), "i64");
    }

    #[test]
    fn test_mlir_memref_type() {
        assert_eq!(
            mlir_memref_type("i64", Some(&[10, 20])),
            "memref<10x20xi64>"
        );
        assert_eq!(mlir_memref_type("f32", None), "memref<?xf32>");
    }

    #[test]
    fn test_mlir_function_type() {
        assert_eq!(
            mlir_function_type(&["i64".to_string()], &["i64".to_string()]),
            "(i64) -> i64"
        );
        assert_eq!(mlir_function_type(&[], &[]), "() -> ()");
    }
}
