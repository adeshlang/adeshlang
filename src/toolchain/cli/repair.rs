//! Installation repair command implementation (`adl repair` / `adesh repair`)

use super::super::expose::{self, Scope};
use super::super::resolver::{detect_system_toolchain, installation_home, resolve};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

pub fn execute_repair_command() {
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    println!("\n{}AdeshLang Installation Auto-Repair Tool{}", bold, reset);
    println!("────────────────────────────────────────────");

    // 1. Resolve or create configuration directories
    if let Ok(appdata) = env::var("LOCALAPPDATA") {
        let appdata_path = PathBuf::from(appdata);
        let adl_dir = appdata_path.join("AdeshLang");
        let pkg_dir = adl_dir.join("packages");
        let cache_dir = adl_dir.join("cache");

        let _ = fs::create_dir_all(&pkg_dir);
        let _ = fs::create_dir_all(&cache_dir);
        println!(
            "  {}✓{} User package & cache directories verified: {}",
            green,
            reset,
            adl_dir.display()
        );
    }

    // 2. Resolve Installation Home Directory Structure
    let home = installation_home().unwrap_or_else(|| {
        let fallback = if let Ok(exe) = env::current_exe() {
            if let Some(parent) = exe.parent() {
                if parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
                    parent.parent().unwrap_or(parent).to_path_buf()
                } else {
                    parent.to_path_buf()
                }
            } else {
                PathBuf::from(".")
            }
        } else {
            PathBuf::from(".")
        };
        fallback
    });

    let lib_dir = home.join("lib");
    let bin_dir = home.join("bin");
    let std_dir = home.join("std");
    let config_dir = home.join("config");
    let toolchain_dir = home.join("toolchain");

    let _ = fs::create_dir_all(&lib_dir);
    let _ = fs::create_dir_all(&bin_dir);
    let _ = fs::create_dir_all(&std_dir);
    let _ = fs::create_dir_all(&config_dir);
    let _ = fs::create_dir_all(&toolchain_dir);
    println!(
        "  {}✓{} Installation Home structure verified: {}",
        green,
        reset,
        home.display()
    );

    // 3. Toolchain Manifest Repair
    let manifest_dest = config_dir.join("toolchain-manifest.json");
    if !manifest_dest.exists() {
        let mut manifest_candidates = vec![
            PathBuf::from("installer")
                .join("manifests")
                .join("toolchain-manifest.json"),
            PathBuf::from("config").join("toolchain-manifest.json"),
        ];
        if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            manifest_candidates.push(
                PathBuf::from(manifest_dir)
                    .join("installer")
                    .join("manifests")
                    .join("toolchain-manifest.json"),
            );
        }
        for cand in manifest_candidates {
            if cand.exists() && cand.is_file() {
                if fs::copy(&cand, &manifest_dest).is_ok() {
                    println!(
                        "  {}✓{} Restored toolchain manifest from {}",
                        green,
                        reset,
                        cand.display()
                    );
                    break;
                }
            }
        }
    }

    // 4. Standard Library (std) Repair
    let std_has_files = fs::read_dir(&std_dir)
        .map(|mut r| r.next().is_some())
        .unwrap_or(false);

    if !std_has_files {
        let mut std_candidates = Vec::new();
        if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            std_candidates.push(PathBuf::from(manifest_dir).join("src").join("stdlib"));
        }
        std_candidates.push(PathBuf::from("src").join("stdlib"));
        std_candidates.push(home.join("src").join("stdlib"));

        let mut copied_std = 0usize;
        for cand in std_candidates {
            if cand.exists() && cand.is_dir() {
                if let Ok(entries) = fs::read_dir(&cand) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("adesh") {
                            if let Some(file_name) = path.file_name() {
                                let dest_file = std_dir.join(file_name);
                                if fs::copy(&path, &dest_file).is_ok() {
                                    copied_std += 1;
                                }
                            }
                        }
                    }
                }
                if copied_std > 0 {
                    println!(
                        "  {}✓{} Restored {} standard library files to {}",
                        green,
                        reset,
                        copied_std,
                        std_dir.display()
                    );
                    break;
                }
            }
        }
    } else {
        println!(
            "  {}✓{} Standard library files verified: {}",
            green,
            reset,
            std_dir.display()
        );
    }

    // 5. Static Runtime Library Repair (adeshlang.lib / libadeshlang.a)
    let lib_name = if cfg!(windows) {
        "adeshlang.lib"
    } else {
        "libadeshlang.a"
    };
    let target_lib = lib_dir.join(lib_name);

    if !target_lib.exists() {
        let mut candidates = Vec::new();
        if let Ok(exe_path) = env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                candidates.push(exe_dir.join(lib_name));
                candidates.push(exe_dir.join("lib").join(lib_name));
                if let Some(parent) = exe_dir.parent() {
                    candidates.push(parent.join("lib").join(lib_name));
                    candidates.push(parent.join("target").join("release").join(lib_name));
                    candidates.push(parent.join("target").join("debug").join(lib_name));
                    candidates.push(
                        parent
                            .join("target")
                            .join("release")
                            .join("deps")
                            .join(lib_name),
                    );
                }
            }
        }
        if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
            let md = PathBuf::from(manifest_dir);
            candidates.push(md.join("target").join("release").join(lib_name));
            candidates.push(md.join("target").join("debug").join(lib_name));
            candidates.push(md.join("lib").join(lib_name));
        }
        candidates.push(PathBuf::from("target").join("release").join(lib_name));
        candidates.push(PathBuf::from("target").join("debug").join(lib_name));
        candidates.push(
            PathBuf::from("dist")
                .join(format!("windows-{}", env::consts::ARCH))
                .join("lib")
                .join(lib_name),
        );

        let mut restored = false;
        for cand in &candidates {
            if cand.exists() && cand.is_file() {
                if fs::copy(cand, &target_lib).is_ok() {
                    println!(
                        "  {}✓{} Restored runtime library {} from {}",
                        green,
                        reset,
                        lib_name,
                        cand.display()
                    );
                    restored = true;
                    break;
                }
            }
        }

        // If not found, attempt to build via cargo if cargo is available in repo
        if !restored {
            if Path::new("Cargo.toml").exists() {
                println!(
                    "  ▶ Compiling missing runtime library via `cargo build --release --lib`..."
                );
                let status = std::process::Command::new("cargo")
                    .args(["build", "--release", "--lib"])
                    .status();
                if let Ok(s) = status {
                    if s.success() {
                        let compiled_lib = PathBuf::from("target").join("release").join(lib_name);
                        if compiled_lib.exists() && fs::copy(&compiled_lib, &target_lib).is_ok() {
                            println!(
                                "  {}✓{} Successfully built and installed {}",
                                green,
                                reset,
                                target_lib.display()
                            );
                            restored = true;
                        }
                    }
                }
            }
        }

        if !restored {
            println!(
                "  {}!{} Runtime library {} could not be auto-located in {}.",
                yellow,
                reset,
                lib_name,
                lib_dir.display()
            );
        }
    } else {
        println!(
            "  {}✓{} Runtime library verified: {}",
            green,
            reset,
            target_lib.display()
        );
    }

    // 6. Toolchain Auto-Repair & Exposure
    let toolchain_ready = match resolve(None) {
        Ok(_) => true,
        Err(_) => false,
    };

    if !toolchain_ready {
        // Check if system Clang/LLVM is detected on PATH or standard directories
        if let Some((clang_path, version)) = detect_system_toolchain() {
            println!(
                "  {}✓{} Detected existing system toolchain: clang {} ({})",
                green,
                reset,
                version.as_deref().unwrap_or("unknown"),
                clang_path.display()
            );
            if let Some(bin_dir) = clang_path.parent() {
                let llvm_root = bin_dir.parent().unwrap_or(bin_dir);
                unsafe {
                    env::set_var("ADESH_TOOLCHAIN", llvm_root);
                }
            }
        } else {
            // Automatically run toolchain installer to download pinned LLVM 18.1.8
            println!(
                "\n▶ Missing LLVM toolchain. Automatically downloading and installing pinned LLVM 18.1.8..."
            );
            super::install::execute_install_command(&["--user".to_string()]);
        }
    }

    // 7. Register Environment Variables & User PATH
    println!("\n▶ Registering environment variables and PATH...");
    let _ = expose::expose(&home, Scope::User);

    // Set session variables
    unsafe {
        env::set_var("ADESH_HOME", &home);
        env::set_var("ADESHLANG_HOME", &home);
    }

    // 8. Re-check doctor health after repair actions
    println!("────────────────────────────────────────────");
    println!(
        "{}All repair actions finished. Running health doctor check...{}\n",
        bold, reset
    );
    super::doctor::execute_doctor_command();
}
