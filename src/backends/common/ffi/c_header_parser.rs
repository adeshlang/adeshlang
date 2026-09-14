use crate::parsing::ast::ExternFunctionDecl;
/// C Header Import Parser (@cImport)
///
/// Parses C header files to extract function declarations and automatically
/// generate equivalent ExternFunctionDecl for FFI usage.
///
/// This is a simplified parser that handles basic C function declarations.
/// For complex headers, consider using bindgen or manual extern declarations.
use std::fs;
use std::path::Path;

/// Parse a C header file and extract function declarations
pub fn parse_c_header(path: &str) -> Result<Vec<ExternFunctionDecl>, String> {
    let file_path = Path::new(path);

    if !file_path.exists() {
        return Err(format!("Header file not found: {}", path));
    }

    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read header file '{}': {}", path, e))?;

    let functions = extract_function_declarations(&content)?;

    Ok(functions)
}

/// Extract function declarations from C header content
fn extract_function_declarations(content: &str) -> Result<Vec<ExternFunctionDecl>, String> {
    let mut declarations = Vec::new();

    // Preprocess: remove comments
    let content = remove_comments(content);

    // Normalize content - join multi-line declarations
    let normalized = normalize_multiline(&content);

    // Simple regex-like parsing for function declarations
    // Pattern: <return_type> <name>(<params>);
    for line in normalized.lines() {
        let trimmed = line.trim();

        // Skip preprocessor directives, typedefs, structs, etc.
        if trimmed.starts_with('#')
            || trimmed.starts_with("typedef")
            || trimmed.starts_with("struct")
            || trimmed.starts_with("enum")
            || trimmed.starts_with("union")
            || trimmed.is_empty()
        {
            continue;
        }

        // Look for function declarations ending with );
        if trimmed.ends_with(");") {
            if let Some(decl) = parse_function_line(trimmed) {
                declarations.push(decl);
            }
        }
    }

    Ok(declarations)
}

/// Normalize multi-line declarations into single lines
fn normalize_multiline(content: &str) -> String {
    let mut result = String::new();
    let mut current_line = String::new();
    let mut in_declaration = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Skip preprocessor and empty lines
        if trimmed.starts_with('#') || trimmed.is_empty() {
            if !current_line.is_empty() {
                result.push_str(&current_line);
                result.push('\n');
                current_line.clear();
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // Check if this could be start of a function declaration
        if !in_declaration
            && (trimmed.contains('(')
                || (!trimmed.starts_with("typedef")
                    && !trimmed.starts_with("struct")
                    && !trimmed.starts_with("enum")))
        {
            in_declaration = true;
        }

        if in_declaration {
            current_line.push(' ');
            current_line.push_str(trimmed);

            // Check if declaration is complete
            if trimmed.ends_with(");") || trimmed.ends_with('}') {
                result.push_str(current_line.trim());
                result.push('\n');
                current_line.clear();
                in_declaration = false;
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    // Add any remaining content
    if !current_line.is_empty() {
        result.push_str(current_line.trim());
        result.push('\n');
    }

    result
}

/// Remove C/C++ style comments
#[allow(clippy::while_let_on_iterator)]
fn remove_comments(content: &str) -> String {
    let mut result = String::new();
    let mut chars = content.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '/' {
            if let Some(&next) = chars.peek() {
                if next == '/' {
                    // Single-line comment: skip until newline
                    chars.next(); // consume '/'
                    while let Some(c) = chars.next() {
                        if c == '\n' {
                            result.push(c);
                            break;
                        }
                    }
                    continue;
                } else if next == '*' {
                    // Multi-line comment: skip until */
                    chars.next(); // consume '*'
                    let mut found_end = false;
                    while let Some(c) = chars.next() {
                        if c == '*' {
                            if let Some(&n) = chars.peek() {
                                if n == '/' {
                                    chars.next(); // consume '/'
                                    found_end = true;
                                    break;
                                }
                            }
                        }
                    }
                    if !found_end {
                        return result; // Unclosed comment
                    }
                    continue;
                }
            }
        }
        result.push(ch);
    }

    result
}

/// Parse a single function declaration line
fn parse_function_line(line: &str) -> Option<ExternFunctionDecl> {
    // Remove trailing );
    let line = line.trim_end_matches(");").trim();

    // Find opening parenthesis
    let paren_pos = line.find('(')?;

    // Split into signature and params
    let signature = &line[..paren_pos].trim();
    let params_str = &line[paren_pos + 1..].trim();

    // Parse signature (return_type function_name)
    let parts: Vec<&str> = signature.split_whitespace().collect();
    if parts.len() < 2 {
        return None;
    }

    // Last part is function name, rest is return type
    let name = parts.last()?.trim_start_matches('*').to_string();
    let ret_type_str = parts[..parts.len() - 1].join(" ");

    // Parse return type
    let ret_type = map_c_type_to_adesh(&ret_type_str);

    // Parse parameters
    let params = parse_c_params(params_str);

    Some(ExternFunctionDecl {
        abi: "C".to_string(),
        name,
        params,
        ret_type,
    })
}

/// Parse C function parameters
fn parse_c_params(params_str: &str) -> Vec<(String, String)> {
    if params_str.trim() == "void" || params_str.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();

    // Split by comma (simple approach, doesn't handle complex types)
    for (i, param) in params_str.split(',').enumerate() {
        let param = param.trim();

        // Parse type and optional name
        let parts: Vec<&str> = param.split_whitespace().collect();
        if parts.is_empty() {
            continue;
        }

        // If last part looks like an identifier, it's the name; otherwise generate one
        let (type_str, name) = if parts.len() > 1 && !parts.last().unwrap().contains('*') {
            let name = parts.last().unwrap().trim_start_matches('*').to_string();
            let type_str = parts[..parts.len() - 1].join(" ");
            (type_str, name)
        } else {
            (parts.join(" "), format!("arg{}", i))
        };

        let adesh_type = map_c_type_to_adesh(&type_str);
        result.push((name, adesh_type));
    }

    result
}

/// Map C type to AdeshLang type string
fn map_c_type_to_adesh(c_type: &str) -> String {
    let c_type = c_type.trim();

    // Handle pointer types
    if c_type.contains('*') {
        let base = c_type.trim_end_matches('*').trim();
        if base == "void" || base == "const void" {
            return "ptr<void>".to_string();
        } else if base == "char" || base == "const char" {
            // char* is typically a string
            return "ptr<u8>".to_string();
        } else {
            let inner = map_c_basic_type(base);
            return format!("ptr<{}>", inner);
        }
    }

    // Basic types
    map_c_basic_type(c_type).to_string()
}

/// Map basic C types to Adesh types
fn map_c_basic_type(c_type: &str) -> &str {
    match c_type.trim() {
        "void" => "void",
        "char" | "signed char" => "i8",
        "unsigned char" => "u8",
        "short" | "short int" | "signed short" => "i16",
        "unsigned short" | "unsigned short int" => "u16",
        "int" | "signed int" | "signed" => "i32",
        "unsigned int" | "unsigned" => "u32",
        "long" | "long int" | "signed long" => "i64",
        "unsigned long" | "unsigned long int" => "u64",
        "long long" | "long long int" | "signed long long" => "i64",
        "unsigned long long" | "unsigned long long int" => "u64",
        "float" => "f32",
        "double" => "f64",
        "bool" | "_Bool" => "bool",
        "size_t" | "ssize_t" => "usize",
        "int8_t" => "i8",
        "int16_t" => "i16",
        "int32_t" => "i32",
        "int64_t" => "i64",
        "uint8_t" => "u8",
        "uint16_t" => "u16",
        "uint32_t" => "u32",
        "uint64_t" => "u64",
        _ => "i32", // Default fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_function() {
        let header = "int add(int a, int b);";
        let decls = extract_function_declarations(header).unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].name, "add");
        assert_eq!(decls[0].params.len(), 2);
    }

    #[test]
    fn test_parse_with_comments() {
        let header = r#"
            // This is a comment
            int foo(int x); /* multi
            line comment */
            void bar(void);
        "#;
        let decls = extract_function_declarations(header).unwrap();
        assert_eq!(decls.len(), 2);
    }

    #[test]
    fn test_pointer_types() {
        let header = "char* get_string(const char* input);";
        let decls = extract_function_declarations(header).unwrap();
        assert_eq!(decls.len(), 1);
        assert_eq!(decls[0].ret_type, "ptr<u8>");
    }
}
