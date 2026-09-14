//! Path resolution utilities
//!
//! This module provides utilities for locating and resolving file paths.

use std::fs;
use std::path::{Path, PathBuf};

/// Try to locate a file by searching `start` recursively for a path
/// that either ends with `target` or has the same filename as `target`.
pub fn find_file_recursive(start: &Path, target: &str) -> Option<PathBuf> {
    let mut stack = vec![start.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let read = fs::read_dir(&dir);
        if read.is_err() {
            continue;
        }
        for e in read.unwrap().filter_map(|r| r.ok()) {
            let p = e.path();
            if p.is_file() {
                if let Some(s) = p.to_str() {
                    if s.ends_with(target) {
                        return Some(p.clone());
                    }
                }
                if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                    if fname == target {
                        return Some(p.clone());
                    }
                }
            } else if p.is_dir() {
                stack.push(p);
            }
        }
    }
    None
}

/// Resolve input path: try the original path first, otherwise attempt to
/// find a matching file by searching the current working directory
/// recursively. Returns (resolved_path, source_contents) or Err(message).
pub fn resolve_input_path(orig_path: &PathBuf) -> Result<(PathBuf, String), String> {
    if let Ok(s) = fs::read_to_string(&orig_path) {
        return Ok((orig_path.clone(), s));
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let target = orig_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| format!("invalid path: {}", orig_path.display()))?;

    // Try recursive search from CWD
    if let Some(found) = find_file_recursive(&cwd, target) {
        if let Ok(s) = fs::read_to_string(&found) {
            return Ok((found, s));
        }
    }

    // As a fallback, try searching from the project root (parent dirs)
    let mut probe = cwd.as_path();
    while let Some(parent) = probe.parent() {
        if let Some(found) = find_file_recursive(parent, target) {
            if let Ok(s) = fs::read_to_string(&found) {
                return Ok((found, s));
            }
        }
        probe = parent;
    }

    Err(format!(
        "failed to read file '{}'. Searched current directory and subdirectories for '{}'",
        orig_path.display(),
        target
    ))
}
