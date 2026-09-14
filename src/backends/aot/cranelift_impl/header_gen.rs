//! C header file generation for Cranelift AOT backend
//!
//! This module handles generation of C header files for exported functions
//! from compiled AdeshLang modules.

use crate::backends::aot::lir::{LirFunction, LirModule, LirType};

/// Generate a C header file for exported functions in a LIR module
pub(crate) fn generate_header(lir_module: &LirModule) -> String {
    let mut header = String::new();

    // Header guard
    header.push_str("#ifndef ADESH_EXPORTS_H\n");
    header.push_str("#define ADESH_EXPORTS_H\n\n");

    // Standard C includes
    header.push_str("#include <stdint.h>\n");
    header.push_str("#include <stdbool.h>\n");
    header.push_str("#include <stddef.h>\n\n");

    // Extern C guard for C++ compatibility
    header.push_str("#ifdef __cplusplus\n");
    header.push_str("extern \"C\" {\n");
    header.push_str("#endif\n\n");

    // Generate function declarations for exported functions
    header.push_str("/* Exported Functions */\n\n");

    for func in &lir_module.functions {
        if func.is_exported && func.name != "main" {
            // Generate function signature
            header.push_str(&lir_function_to_c_signature(func));
            header.push_str(";\n\n");
        }
    }

    // Close extern C guard
    header.push_str("#ifdef __cplusplus\n");
    header.push_str("}\n");
    header.push_str("#endif\n\n");

    // Close header guard
    header.push_str("#endif /* ADESH_EXPORTS_H */\n");

    header
}

/// Convert an LIR function to a C function signature
fn lir_function_to_c_signature(func: &LirFunction) -> String {
    let return_type = lir_type_to_c_type(&func.ret_type);
    let mut sig = format!("{} {}(", return_type, func.name);

    if func.params.is_empty() {
        sig.push_str("void");
    } else {
        for (i, (param_name, param_type)) in func.params.iter().enumerate() {
            if i > 0 {
                sig.push_str(", ");
            }
            let c_type = lir_type_to_c_type(param_type);
            sig.push_str(&format!("{} {}", c_type, param_name));
        }
    }

    sig.push(')');
    sig
}

/// Map LIR type to C type string
fn lir_type_to_c_type(ty: &LirType) -> String {
    match ty {
        LirType::I8 => "int32_t".to_string(),
        LirType::I16 => "int32_t".to_string(),
        LirType::I32 => "int32_t".to_string(),
        LirType::I64 => "int64_t".to_string(),
        LirType::I128 => "int64_t".to_string(),
        LirType::U8 => "int32_t".to_string(),
        LirType::U16 => "int32_t".to_string(),
        LirType::U32 => "int32_t".to_string(),
        LirType::U64 => "int64_t".to_string(),
        LirType::U128 => "int64_t".to_string(),
        LirType::F32 => "float".to_string(),
        LirType::F64 => "double".to_string(),
        LirType::Bool => "int32_t".to_string(),
        LirType::Ptr => "void*".to_string(),
        LirType::Void => "void".to_string(),
    }
}
