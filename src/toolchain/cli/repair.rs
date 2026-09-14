//! Installation repair command implementation (`adl repair`)

use std::env;
use std::fs;
use std::path::PathBuf;

pub fn execute_repair_command() {
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    println!("\n{}AdeshLang Installation Repair Tool{}", bold, reset);
    println!("────────────────────────────────────────────");

    // 1. Resolve or create configuration directories
    if let Ok(appdata) = env::var("LOCALAPPDATA") {
        let appdata_path = PathBuf::from(appdata);
        let adl_dir = appdata_path.join("AdeshLang");
        let pkg_dir = adl_dir.join("packages");
        let cache_dir = adl_dir.join("cache");

        if let Err(e) = fs::create_dir_all(&pkg_dir) {
            println!(
                "  {}!{} Failed to create package directory: {}",
                yellow, reset, e
            );
        } else {
            println!(
                "  {}✓{} Package directory verified: {}",
                green,
                reset,
                pkg_dir.display()
            );
        }

        if let Err(e) = fs::create_dir_all(&cache_dir) {
            println!(
                "  {}!{} Failed to create cache directory: {}",
                yellow, reset, e
            );
        } else {
            println!(
                "  {}✓{} Cache directory verified: {}",
                green,
                reset,
                cache_dir.display()
            );
        }
    }

    // 2. Local Project Cache Verification
    let local_cache = PathBuf::from(".adesh_cache").join("aot");
    if let Err(e) = fs::create_dir_all(&local_cache) {
        println!(
            "  {}!{} Failed to create local project cache directory: {}",
            yellow, reset, e
        );
    } else {
        println!(
            "  {}✓{} Project build cache verified: {}",
            green,
            reset,
            local_cache.display()
        );
    }

    // 2. Set environment variables for current session if the install home is missing
    //    ADESH_HOME is canonical; ADESHLANG_HOME is kept as a legacy alias.
    if env::var("ADESH_HOME").is_err() && env::var("ADESHLANG_HOME").is_err() {
        if let Ok(exe_path) = env::current_exe() {
            if let Some(parent) = exe_path.parent() {
                let home_path = if parent.file_name().and_then(|n| n.to_str()) == Some("bin") {
                    parent.parent().unwrap_or(parent)
                } else {
                    parent
                };
                unsafe {
                    env::set_var("ADESH_HOME", &home_path);
                    env::set_var("ADESHLANG_HOME", home_path);
                }
                println!(
                    "  {}✓{} Configured session ADESH_HOME to {}",
                    green,
                    reset,
                    home_path.display()
                );
            }
        }
    }

    // 3. Re-check doctor health after repair actions
    println!("────────────────────────────────────────────");
    println!(
        "{}Repair actions completed. Running diagnostic doctor check...{}\n",
        bold, reset
    );
    super::doctor::execute_doctor_command();
}
