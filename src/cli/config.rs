//! FFI configuration module
//!
//! This module provides FFI-related configuration utilities.

use crate::cli::RuntimeConfig;
use std::path::PathBuf;

/// Apply FFI-related CLI configuration to the global FFI registry
pub fn apply_ffi_cli_config(config: &RuntimeConfig) {
    use crate::backends::ffi_import::{add_link_lib, add_search_path, set_debug};
    set_debug(config.ffi_debug);
    for p in &config.lib_paths {
        add_search_path(PathBuf::from(p));
    }
    for l in &config.link_libs {
        add_link_lib(l.clone());
    }

    // Automatically load lib names from project's own adesh.adl
    if let Ok(manifest) = crate::ecosystem::manifest::Manifest::load("adesh.adl") {
        if let Some(libs) = manifest.project_libs() {
            for l in libs {
                add_link_lib(l);
            }
        }
    }

    // Automatically search and register adl_modules search paths for FFI/native binaries
    if let Ok(cwd) = std::env::current_dir() {
        let mut probe = Some(cwd.as_path());
        while let Some(dir) = probe {
            let adl_modules = dir.join("adl_modules");
            if adl_modules.exists() && adl_modules.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&adl_modules) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.is_dir() {
                            add_search_path(path.clone());
                            for sub in &[
                                "bin",
                                "lib",
                                "target/release",
                                "target/debug",
                                "build",
                                "src",
                            ] {
                                let sub_path = path.join(sub);
                                if sub_path.exists() && sub_path.is_dir() {
                                    add_search_path(sub_path);
                                }
                            }
                            // Auto-load libs defined in this dependency's manifest
                            let manifest_path = path.join("adesh.adl");
                            if manifest_path.exists() {
                                if let Ok(m) =
                                    crate::ecosystem::manifest::Manifest::load(&manifest_path)
                                {
                                    if let Some(libs) = m.project_libs() {
                                        for l in libs {
                                            add_link_lib(l);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                break;
            }
            probe = dir.parent();
        }
    }

    // Apply memory policy for embedded mode
    if config.embedded {
        crate::memory::set_memory_policy(crate::memory::MemoryPolicy::embedded());
        eprintln!("🛰️ Embedded mode enabled: heap and ARC disabled, stack + arena only");
    } else {
        crate::memory::set_memory_policy(crate::memory::MemoryPolicy::standard());
    }
}
