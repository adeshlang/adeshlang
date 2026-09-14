//! Compile directive utilities
//!
//! This module provides utilities for detecting and stripping
//! compile directives from source code.

/// Detect compile directive in source code
pub fn detect_compile_directive(src: &str) -> Option<String> {
    let clean = src.strip_prefix("\u{feff}").unwrap_or(src);
    for line in clean.lines() {
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        if t.starts_with("//") {
            continue;
        }
        if t.starts_with("@compile") {
            let rest = t["@compile".len()..].trim();
            if rest.is_empty() {
                return Some("bytecode".to_string());
            }
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() > 0 {
                return Some(parts[0].to_string());
            }
            return Some("bytecode".to_string());
        }
        break;
    }
    None
}

/// Strip compile directive from source code
pub fn strip_compile_directive(src: &str) -> String {
    let clean = src.strip_prefix("\u{feff}").unwrap_or(src);
    let mut lines = clean.lines();
    if let Some(first) = lines.next() {
        let t = first.trim();
        if t.starts_with("@compile") {
            return lines.collect::<Vec<&str>>().join("\n");
        }
    }
    clean.to_string()
}
