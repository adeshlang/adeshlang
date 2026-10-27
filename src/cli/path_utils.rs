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

/// Resolve input path: check if the specified file exists (either directly
/// or by appending `.adesh`).
/// Does NOT search parent drives/directories recursively. Returns (resolved_path, source_contents)
/// or Err(message) immediately.
pub fn resolve_input_path(orig_path: &PathBuf) -> Result<(PathBuf, String), String> {
    if orig_path.is_file() {
        if let Ok(s) = fs::read_to_string(orig_path) {
            return Ok((orig_path.clone(), s));
        }
    }

    // If path doesn't have an extension, try appending .adesh
    if orig_path.extension().is_none() {
        let mut with_adesh = orig_path.clone();
        with_adesh.set_extension("adesh");
        if with_adesh.is_file() {
            if let Ok(s) = fs::read_to_string(&with_adesh) {
                return Ok((with_adesh, s));
            }
        }
    }

    Err(format!("error: file not found: '{}'", orig_path.display()))
}
