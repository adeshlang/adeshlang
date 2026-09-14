//! Standard Library Safeguards
//!
//! This module provides input validation, bounds checking, and safety utilities
//! for the language standard library functions.
//!
//! Features:
//! - Array bounds checking with clear error messages
//! - String operation validation
//! - Path sanitization for fs module
//! - URL validation for http module
//! - Numeric input validation

use std::path::Path;

// ============================================
// ARRAY BOUNDS CHECKING
// ============================================

/// Check if index is within array bounds
/// Returns Ok(usize) if valid, Err with descriptive message if not
pub fn check_array_bounds(len: usize, index: i64, operation: &str) -> Result<usize, String> {
    if index < 0 {
        // Support negative indexing
        let adjusted = len as i64 + index;
        if adjusted < 0 {
            return Err(format!(
                "{}: negative index {} is out of bounds for array of length {}",
                operation, index, len
            ));
        }
        Ok(adjusted as usize)
    } else {
        let idx = index as usize;
        if idx >= len {
            return Err(format!(
                "{}: index {} is out of bounds for array of length {}",
                operation, idx, len
            ));
        }
        Ok(idx)
    }
}

/// Check if a slice range is valid for an array
pub fn check_slice_bounds(
    len: usize,
    start: Option<i64>,
    end: Option<i64>,
    operation: &str,
) -> Result<(usize, usize), String> {
    // Handle start index
    let start_idx = match start {
        Some(s) => {
            if s < 0 {
                let adjusted = len as i64 + s;
                if adjusted < 0 { 0 } else { adjusted as usize }
            } else {
                (s as usize).min(len)
            }
        }
        None => 0,
    };

    // Handle end index
    let end_idx = match end {
        Some(e) => {
            if e < 0 {
                let adjusted = len as i64 + e;
                if adjusted < 0 { 0 } else { adjusted as usize }
            } else {
                (e as usize).min(len)
            }
        }
        None => len,
    };

    // Validate range
    if start_idx > end_idx {
        return Err(format!(
            "{}: invalid slice range [{}:{}] for array of length {}",
            operation, start_idx, end_idx, len
        ));
    }

    Ok((start_idx, end_idx))
}

// ============================================
// STRING OPERATION VALIDATION
// ============================================

/// Check if index is valid for string character access
pub fn check_string_index(s: &str, index: i64, operation: &str) -> Result<usize, String> {
    let char_count = s.chars().count();
    check_array_bounds(char_count, index, operation)
}

/// Validate substring indices
pub fn check_substring_bounds(
    s: &str,
    start: Option<i64>,
    end: Option<i64>,
    operation: &str,
) -> Result<(usize, usize), String> {
    let char_count = s.chars().count();
    check_slice_bounds(char_count, start, end, operation)
}

/// Get character at index with bounds checking
pub fn get_char_at(s: &str, index: i64) -> Result<char, String> {
    let idx = check_string_index(s, index, "charAt")?;
    s.chars()
        .nth(idx)
        .ok_or_else(|| format!("charAt: index {} not found", idx))
}

/// Get substring with bounds checking
pub fn get_substring(s: &str, start: Option<i64>, end: Option<i64>) -> Result<String, String> {
    let (start_idx, end_idx) = check_substring_bounds(s, start, end, "substring")?;
    Ok(s.chars()
        .skip(start_idx)
        .take(end_idx - start_idx)
        .collect())
}

// ============================================
// PATH SANITIZATION
// ============================================

/// Sanitize a file path to prevent directory traversal attacks
pub fn sanitize_path(path: &str, base_dir: Option<&Path>) -> Result<std::path::PathBuf, String> {
    let path = Path::new(path);

    // Check for null bytes
    if path.to_string_lossy().contains('\0') {
        return Err("Path contains null byte".to_string());
    }

    // Normalize the path
    let mut normalized = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(c) => normalized.push(c),
            std::path::Component::CurDir => {} // Skip "."
            std::path::Component::ParentDir => {
                // Allow ".." only if we have path components to go back
                if !normalized.pop() {
                    return Err("Path escapes base directory".to_string());
                }
            }
            std::path::Component::RootDir => {
                if base_dir.is_some() {
                    // If we have a base dir, don't allow absolute paths
                    return Err("Absolute paths not allowed".to_string());
                }
                normalized.push(component);
            }
            std::path::Component::Prefix(p) => {
                normalized.push(p.as_os_str());
            }
        }
    }

    // If base directory is specified, join with it
    if let Some(base) = base_dir {
        Ok(base.join(&normalized))
    } else {
        Ok(normalized)
    }
}

/// Validate that a path is safe for file operations
pub fn validate_file_path(path: &str) -> Result<(), String> {
    // Check for empty path
    if path.is_empty() {
        return Err("Path cannot be empty".to_string());
    }

    // Check path length
    if path.len() > 4096 {
        return Err("Path too long (max 4096 characters)".to_string());
    }

    // Check for suspicious patterns
    let suspicious = [
        "/etc/passwd",
        "/etc/shadow",
        "~/.ssh",
        "../../../",
        "..\\..\\..\\",
    ];

    let path_lower = path.to_lowercase();
    for pattern in &suspicious {
        if path_lower.contains(pattern) {
            return Err(format!("Suspicious path pattern detected: {}", pattern));
        }
    }

    Ok(())
}

// ============================================
// URL VALIDATION
// ============================================

/// Validate a URL for HTTP requests
pub fn validate_url(url: &str) -> Result<(), String> {
    // Check for empty URL
    if url.is_empty() {
        return Err("URL cannot be empty".to_string());
    }

    // Check URL length
    if url.len() > 8192 {
        return Err("URL too long (max 8192 characters)".to_string());
    }

    // Basic protocol check
    let url_lower = url.to_lowercase();
    if !url_lower.starts_with("http://") && !url_lower.starts_with("https://") {
        return Err("URL must start with http:// or https://".to_string());
    }

    // Check for suspicious patterns
    if url.contains("..") {
        return Err("URL contains suspicious '..' pattern".to_string());
    }

    // Check for localhost/internal network (configurable)
    // Check for localhost/internal network access
    // This is a security measure that can be optionally disabled
    // For SSRF protection in production environments
    let blocked_hosts = [
        "localhost",
        "127.0.0.1",
        "0.0.0.0",
        "::1",
        "169.254.", // Link-local
        "192.168.", // Private network
        "10.",      // Private network
    ];

    #[cfg(not(feature = "allow_internal_network"))]
    for host in &blocked_hosts {
        if url_lower.contains(host) {
            return Err(format!(
                "Access to internal network address '{}' is blocked for security reasons",
                host
            ));
        }
    }

    // When allow_internal_network feature is enabled, skip the check
    #[cfg(feature = "allow_internal_network")]
    let _ = blocked_hosts; // Suppress unused warning

    Ok(())
}

// ============================================
// NUMERIC VALIDATION
// ============================================

/// Validate a number is within a specified range
pub fn validate_number_range(value: f64, min: f64, max: f64, name: &str) -> Result<f64, String> {
    if value.is_nan() {
        return Err(format!("{} cannot be NaN", name));
    }
    if value < min {
        return Err(format!("{} must be >= {} (got {})", name, min, value));
    }
    if value > max {
        return Err(format!("{} must be <= {} (got {})", name, max, value));
    }
    Ok(value)
}

/// Validate an integer is within a specified range
pub fn validate_integer_range(value: i64, min: i64, max: i64, name: &str) -> Result<i64, String> {
    if value < min {
        return Err(format!("{} must be >= {} (got {})", name, min, value));
    }
    if value > max {
        return Err(format!("{} must be <= {} (got {})", name, max, value));
    }
    Ok(value)
}

/// Ensure a number is a positive integer
pub fn validate_positive_integer(value: f64, name: &str) -> Result<usize, String> {
    if value < 0.0 {
        return Err(format!("{} must be non-negative (got {})", name, value));
    }
    if value != value.trunc() {
        return Err(format!("{} must be an integer (got {})", name, value));
    }
    if value > usize::MAX as f64 {
        return Err(format!("{} is too large", name));
    }
    Ok(value as usize)
}

// ============================================
// TYPE VALIDATION
// ============================================

/// Expected type for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpectedType {
    Number,
    String,
    Bool,
    Array,
    Object,
    Function,
    Null,
    Any,
}

impl std::fmt::Display for ExpectedType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExpectedType::Number => write!(f, "Number"),
            ExpectedType::String => write!(f, "String"),
            ExpectedType::Bool => write!(f, "Bool"),
            ExpectedType::Array => write!(f, "Array"),
            ExpectedType::Object => write!(f, "Object"),
            ExpectedType::Function => write!(f, "Function"),
            ExpectedType::Null => write!(f, "null"),
            ExpectedType::Any => write!(f, "any"),
        }
    }
}

/// Generate a type mismatch error message
pub fn type_error(expected: ExpectedType, got: &str, arg_name: &str) -> String {
    format!("Expected {} for '{}', got {}", expected, arg_name, got)
}

/// Generate an argument count error message
pub fn arg_count_error(func_name: &str, expected: usize, got: usize) -> String {
    format!(
        "{}() expected {} argument{}, got {}",
        func_name,
        expected,
        if expected == 1 { "" } else { "s" },
        got
    )
}

/// Generate an argument count range error message
pub fn arg_count_range_error(func_name: &str, min: usize, max: usize, got: usize) -> String {
    format!(
        "{}() expected {}-{} arguments, got {}",
        func_name, min, max, got
    )
}

// ============================================
// TESTS
// ============================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_array_bounds_checking() {
        // Valid index
        assert_eq!(check_array_bounds(5, 0, "test"), Ok(0));
        assert_eq!(check_array_bounds(5, 4, "test"), Ok(4));

        // Out of bounds
        assert!(check_array_bounds(5, 5, "test").is_err());
        assert!(check_array_bounds(5, 10, "test").is_err());

        // Negative index
        assert_eq!(check_array_bounds(5, -1, "test"), Ok(4));
        assert_eq!(check_array_bounds(5, -5, "test"), Ok(0));
        assert!(check_array_bounds(5, -6, "test").is_err());
    }

    #[test]
    fn test_slice_bounds() {
        // Valid slice
        let (start, end) = check_slice_bounds(5, Some(1), Some(3), "test").unwrap();
        assert_eq!(start, 1);
        assert_eq!(end, 3);

        // Full slice
        let (start, end) = check_slice_bounds(5, None, None, "test").unwrap();
        assert_eq!(start, 0);
        assert_eq!(end, 5);

        // Negative indices
        let (start, end) = check_slice_bounds(5, Some(-3), Some(-1), "test").unwrap();
        assert_eq!(start, 2);
        assert_eq!(end, 4);
    }

    #[test]
    fn test_path_sanitization() {
        // Normal path
        assert!(sanitize_path("file.txt", None).is_ok());
        assert!(sanitize_path("subdir/file.txt", None).is_ok());

        // With base directory
        let base = Path::new("/home/user");
        let result = sanitize_path("docs/file.txt", Some(base)).unwrap();
        assert!(result.starts_with("/home/user"));

        // Directory traversal (blocked with base)
        assert!(sanitize_path("../../etc/passwd", Some(base)).is_err());
    }

    #[test]
    fn test_url_validation() {
        // Valid URLs
        assert!(validate_url("https://example.com").is_ok());
        assert!(validate_url("http://example.com/path?query=1").is_ok());

        // Invalid URLs
        assert!(validate_url("").is_err());
        assert!(validate_url("ftp://example.com").is_err());

        // Internal network access blocked by default (SSRF protection)
        #[cfg(not(feature = "allow_internal_network"))]
        {
            assert!(validate_url("http://localhost/api").is_err());
            assert!(validate_url("http://127.0.0.1/api").is_err());
            assert!(validate_url("http://192.168.1.1/api").is_err());
        }
    }

    #[test]
    fn test_number_validation() {
        // Valid range
        assert_eq!(validate_number_range(5.0, 0.0, 10.0, "test"), Ok(5.0));

        // Out of range
        assert!(validate_number_range(-1.0, 0.0, 10.0, "test").is_err());
        assert!(validate_number_range(11.0, 0.0, 10.0, "test").is_err());

        // NaN
        assert!(validate_number_range(f64::NAN, 0.0, 10.0, "test").is_err());

        // Positive integer
        assert_eq!(validate_positive_integer(5.0, "test"), Ok(5));
        assert!(validate_positive_integer(-1.0, "test").is_err());
        assert!(validate_positive_integer(5.5, "test").is_err());
    }
}
