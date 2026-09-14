//! Editor launcher module for `adesh editor` command
//!
//! Locates and launches the `adesh-editor` binary as a child process.

use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Get user home directory in a cross-platform manner without extra dependencies
fn user_home_dir() -> Option<PathBuf> {
    if let Ok(home) = env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    if let Ok(user_profile) = env::var("USERPROFILE") {
        if !user_profile.is_empty() {
            return Some(PathBuf::from(user_profile));
        }
    }
    None
}

/// Locate the `adesh-editor` binary across environment, executable directory,
/// cargo target directories, Adesh installation directories, and system PATH.
pub fn find_editor_binary() -> Option<PathBuf> {
    let exe_name = if cfg!(windows) {
        "adesh-editor.exe"
    } else {
        "adesh-editor"
    };

    // 1. Explicit env var
    if let Ok(env_path) = env::var("ADESH_EDITOR_BINARY") {
        let p = PathBuf::from(&env_path);
        if p.is_file() {
            return Some(p);
        }
    }

    // 2. Directory of current executable
    if let Ok(current_exe) = env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            let candidate = exe_dir.join(exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }

            // Check sibling cargo build target folders (target/debug, target/release)
            if let Some(parent) = exe_dir.parent() {
                let debug_cand = parent.join("debug").join(exe_name);
                if debug_cand.is_file() {
                    return Some(debug_cand);
                }
                let release_cand = parent.join("release").join(exe_name);
                if release_cand.is_file() {
                    return Some(release_cand);
                }
            }
        }
    }

    // 3. Project root target directory if running from repo
    if let Ok(cwd) = env::current_dir() {
        let debug_cand = cwd.join("target").join("debug").join(exe_name);
        if debug_cand.is_file() {
            return Some(debug_cand);
        }
        let release_cand = cwd.join("target").join("release").join(exe_name);
        if release_cand.is_file() {
            return Some(release_cand);
        }
    }

    // 4. Home directory ~/.adesh/bin/
    if let Some(home) = user_home_dir() {
        let install_cand = home.join(".adesh").join("bin").join(exe_name);
        if install_cand.is_file() {
            return Some(install_cand);
        }
    }

    // 5. System PATH
    if let Ok(path_var) = env::var("PATH") {
        for path_entry in env::split_paths(&path_var) {
            let candidate = path_entry.join(exe_name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Execute `adesh editor` command
pub fn execute_editor_command(args: &[String]) {
    let editor_binary = match find_editor_binary() {
        Some(bin) => bin,
        None => {
            eprintln!("Adesh Editor binary not found.");
            eprintln!();
            eprintln!("Expected:");
            eprintln!("  adesh-editor");
            eprintln!();
            eprintln!("Please ensure AdeshLang TUI Editor is installed and available in PATH,");
            eprintln!("or build it with: cargo build --package adesh-editor");
            std::process::exit(1);
        }
    };

    let mut command = Command::new(&editor_binary);
    command.args(args);

    match command.status() {
        Ok(status) => {
            let code = status.code().unwrap_or(0);
            std::process::exit(code);
        }
        Err(e) => {
            eprintln!("Failed to execute editor binary '{:?}': {}", editor_binary, e);
            std::process::exit(1);
        }
    }
}
