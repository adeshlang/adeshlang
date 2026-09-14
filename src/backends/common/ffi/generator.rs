/// FFI (Foreign Function Interface) generator for Adesh
///
/// This module generates C ABI-compatible headers and bindings
/// for exporting Adesh functions to C and importing C functions into Adesh.
use std::collections::HashMap;

/// C type representation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CType {
    Void,
    I8,
    U8,
    I16,
    U16,
    I32,
    U32,
    I64,
    U64,
    F32,
    F64,
    Bool,
    Ptr(Box<CType>),
    ConstPtr(Box<CType>),
    Struct(String),
    Unknown(String),
}

impl CType {
    /// Get the C type name as a string
    pub fn to_c_string(&self) -> String {
        match self {
            CType::Void => "void".to_string(),
            CType::I8 => "int8_t".to_string(),
            CType::U8 => "uint8_t".to_string(),
            CType::I16 => "int16_t".to_string(),
            CType::U16 => "uint16_t".to_string(),
            CType::I32 => "int32_t".to_string(),
            CType::U32 => "uint32_t".to_string(),
            CType::I64 => "int64_t".to_string(),
            CType::U64 => "uint64_t".to_string(),
            CType::F32 => "float".to_string(),
            CType::F64 => "double".to_string(),
            CType::Bool => "bool".to_string(),
            CType::Ptr(inner) => format!("{}*", inner.to_c_string()),
            CType::ConstPtr(inner) => format!("const {}*", inner.to_c_string()),
            CType::Struct(name) => format!("struct {}", name),
            CType::Unknown(name) => name.clone(),
        }
    }
}

/// Function signature for FFI binding
#[derive(Debug, Clone)]
pub struct FfiFunction {
    /// Function name
    pub name: String,
    /// Return type
    pub return_type: CType,
    /// Function parameters (name, type)
    pub parameters: Vec<(String, CType)>,
    /// Is this function exported from Adesh?
    pub is_export: bool,
    /// Is this an extern C import?
    pub is_extern: bool,
    /// Documentation comment
    pub doc: Option<String>,
}

/// FFI module information
#[derive(Debug, Clone)]
pub struct FfiModule {
    /// Module name (from filename)
    pub name: String,
    /// Module description/docs
    pub description: Option<String>,
    /// Exported functions
    pub exports: Vec<FfiFunction>,
    /// Imported C functions
    pub imports: Vec<FfiFunction>,
    /// Struct definitions
    pub structs: HashMap<String, Vec<(String, CType)>>,
}

impl FfiModule {
    /// Create a new FFI module
    pub fn new(name: String) -> Self {
        FfiModule {
            name,
            description: None,
            exports: Vec::new(),
            imports: Vec::new(),
            structs: HashMap::new(),
        }
    }

    /// Add an exported function
    pub fn add_export(&mut self, func: FfiFunction) {
        self.exports.push(func);
    }

    /// Add an imported C function
    pub fn add_import(&mut self, func: FfiFunction) {
        self.imports.push(func);
    }

    /// Add a struct definition
    pub fn add_struct(&mut self, name: String, fields: Vec<(String, CType)>) {
        self.structs.insert(name, fields);
    }

    /// Generate C header file content
    pub fn generate_header(&self) -> String {
        let guard = format!("ADESH_{}_H", self.name.to_uppercase());
        let mut header = String::new();

        // Header guard
        header.push_str(&format!("#ifndef {}\n", guard));
        header.push_str(&format!("#define {}\n\n", guard));

        // Includes
        header.push_str("#include <stdint.h>\n");
        header.push_str("#include <stdbool.h>\n");
        header.push_str("#include <stddef.h>\n\n");

        // Extern C guard
        header.push_str("#ifdef __cplusplus\n");
        header.push_str("extern \"C\" {\n");
        header.push_str("#endif\n\n");

        // Module documentation
        if let Some(doc) = &self.description {
            header.push_str("/**\n");
            for line in doc.lines() {
                header.push_str(&format!(" * {}\n", line));
            }
            header.push_str(" */\n\n");
        }

        // Struct definitions
        if !self.structs.is_empty() {
            header.push_str("/* Data Structures */\n");
            for (struct_name, fields) in &self.structs {
                header.push_str("typedef struct {\n");
                for (field_name, field_type) in fields {
                    header.push_str(&format!(
                        "    {} {};\n",
                        field_type.to_c_string(),
                        field_name
                    ));
                }
                header.push_str(&format!("}} {};\n\n", struct_name));
            }
            header.push('\n');
        }

        // Exported functions
        if !self.exports.is_empty() {
            header.push_str("/* Exported Functions from Adesh */\n");
            for func in &self.exports {
                // Documentation
                if let Some(doc) = &func.doc {
                    header.push_str("/**\n");
                    for line in doc.lines() {
                        header.push_str(&format!(" * {}\n", line));
                    }
                    header.push_str(" */\n");
                }

                // Function signature
                header.push_str(&func.to_c_string());
                header.push_str(";\n\n");
            }
        }

        // Imported functions (declarations)
        if !self.imports.is_empty() {
            header.push_str("/* Imported C Functions */\n");
            for func in &self.imports {
                // Function signature (extern is implied in headers)
                header.push_str(&func.to_c_string());
                header.push_str(";\n\n");
            }
        }

        // Close extern C guard
        header.push_str("#ifdef __cplusplus\n");
        header.push_str("}\n");
        header.push_str("#endif\n\n");

        // Close header guard
        header.push_str(&format!("#endif /* {} */\n", guard));

        header
    }

    /// Generate a binding module for importing the library
    pub fn generate_import_bindings(&self) -> String {
        let mut bindings = String::new();

        bindings.push_str(&format!(
            "// Auto-generated FFI bindings for {}\n",
            self.name
        ));
        bindings.push_str("// This file allows importing the compiled library from C\n\n");

        if !self.exports.is_empty() {
            bindings.push_str("// Function declarations from the Adesh library\n");
            for func in &self.exports {
                bindings.push_str(&format!(
                    "extern {} {}(",
                    func.return_type.to_c_string(),
                    func.name
                ));

                // Parameters
                if func.parameters.is_empty() {
                    bindings.push_str("void");
                } else {
                    for (i, (param_name, param_type)) in func.parameters.iter().enumerate() {
                        if i > 0 {
                            bindings.push_str(", ");
                        }
                        bindings.push_str(&format!("{} {}", param_type.to_c_string(), param_name));
                    }
                }

                bindings.push_str(");\n");
            }
            bindings.push('\n');
        }

        bindings
    }

    /// Generate example C code that uses the exported functions
    pub fn generate_example(&self) -> String {
        let mut example = String::new();

        example.push_str("// Example C program using Adesh library\n");
        example.push_str(&format!(
            "// Link against: lib{}.so (Linux) or {}.dll (Windows)\n\n",
            self.name, self.name
        ));

        example.push_str("#include <stdio.h>\n");
        example.push_str(&format!("#include \"{}.h\"\n\n", self.name));

        example.push_str("int main() {\n");

        if let Some(func) = self.exports.first() {
            match func.return_type {
                CType::Void => {
                    example.push_str(&format!("    {}(", func.name));
                    if !func.parameters.is_empty() {
                        example.push_str("/* parameters */");
                    }
                    example.push_str(");\n");
                }
                _ => {
                    example.push_str(&format!(
                        "    {} result = {}(",
                        func.return_type.to_c_string(),
                        func.name
                    ));
                    if !func.parameters.is_empty() {
                        example.push_str("/* parameters */");
                    }
                    example.push_str(");\n");
                    example.push_str("    printf(\"%lld\\n\", (long long)result);\n");
                }
            }
        }

        example.push_str("    return 0;\n");
        example.push_str("}\n");

        example
    }
}

impl FfiFunction {
    /// Convert function to C signature
    pub fn to_c_string(&self) -> String {
        let mut sig = String::new();

        // Return type
        sig.push_str(&self.return_type.to_c_string());
        sig.push(' ');

        // Function name
        sig.push_str(&self.name);
        sig.push('(');

        // Parameters
        if self.parameters.is_empty() {
            sig.push_str("void");
        } else {
            for (i, (param_name, param_type)) in self.parameters.iter().enumerate() {
                if i > 0 {
                    sig.push_str(", ");
                }
                sig.push_str(&format!("{} {}", param_type.to_c_string(), param_name));
            }
        }

        sig.push(')');

        sig
    }

    /// Create a new exported function
    pub fn new_export(name: String, return_type: CType, parameters: Vec<(String, CType)>) -> Self {
        FfiFunction {
            name,
            return_type,
            parameters,
            is_export: true,
            is_extern: false,
            doc: None,
        }
    }

    /// Create a new imported C function
    pub fn new_import(name: String, return_type: CType, parameters: Vec<(String, CType)>) -> Self {
        FfiFunction {
            name,
            return_type,
            parameters,
            is_export: false,
            is_extern: true,
            doc: None,
        }
    }

    /// Set documentation
    pub fn with_doc(mut self, doc: String) -> Self {
        self.doc = Some(doc);
        self
    }
}

/// Helper function to generate FFI from AST
pub fn generate_ffi_module(
    name: &str,
    exported_functions: Vec<(String, String, Vec<String>)>, // (name, return_type, param_types)
) -> FfiModule {
    let mut module = FfiModule::new(name.to_string());

    for (func_name, _return_type, _param_types) in exported_functions {
        let func = FfiFunction::new_export(
            func_name,
            CType::I64, // Default return type
            vec![],     // Will be populated from AST
        );
        module.add_export(func);
    }

    module
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_type_to_string() {
        assert_eq!(CType::I32.to_c_string(), "int32_t");
        assert_eq!(CType::F64.to_c_string(), "double");
        assert_eq!(CType::Void.to_c_string(), "void");
    }

    #[test]
    fn test_ffi_function_signature() {
        let func = FfiFunction::new_export(
            "add".to_string(),
            CType::I32,
            vec![("a".to_string(), CType::I32), ("b".to_string(), CType::I32)],
        );

        let sig = func.to_c_string();
        assert!(sig.contains("add"));
        assert!(sig.contains("int32_t"));
    }

    #[test]
    fn test_ffi_module_header_generation() {
        let mut module = FfiModule::new("test".to_string());
        module.add_export(FfiFunction::new_export(
            "hello".to_string(),
            CType::Void,
            vec![],
        ));

        let header = module.generate_header();
        assert!(header.contains("ADESH_TEST_H"));
        assert!(header.contains("hello"));
        assert!(header.contains("void"));
    }
}
