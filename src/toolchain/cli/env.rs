//! Environment diagnostic command implementation (`adl env`)

use super::super::resolver::{bin_directory, bundled_root, installation_home, resolve};
use std::env;
use std::path::PathBuf;

/// Read a canonical `ADESH_*` environment variable, falling back to the
/// legacy `ADESHLANG_*` spelling. Returns `None` when neither is set.
fn env_var_alias(primary: &str, legacy: &str) -> Option<String> {
    env::var(primary).ok().or_else(|| env::var(legacy).ok())
}

pub fn execute_env_command() {
    execute_env_command_with_format(false, false);
}

pub fn execute_env_command_with_format(json: bool, shell: bool) {
    let home = installation_home();
    let toolchain = bundled_root();
    if json {
        let value = serde_json::json!({
            "version": env!("CARGO_PKG_VERSION"),
            "os": env::consts::OS,
            "architecture": env::consts::ARCH,
            "home": home.as_ref().map(|p| p.display().to_string()),
            "toolchain": toolchain.as_ref().map(|p| p.display().to_string()),
            "llvm": env::var_os("ADESH_LLVM").map(|p| PathBuf::from(p).display().to_string()),
            "bin": bin_directory().map(|p| p.display().to_string()),
            "path_entry": bin_directory().map(|p| p.display().to_string()),
            "active_toolchain": resolve(None).ok().map(|tc| if tc.bundled { "bundled" } else { "system" }),
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&value).expect("JSON serialization cannot fail")
        );
        return;
    }
    if shell {
        let Some(home) = home else {
            eprintln!("Unable to determine AdeshLang installation home");
            return;
        };
        let bin = bin_directory().unwrap_or_else(|| home.join("bin")).display().to_string();
        let home = home.display().to_string();
        match env::var("SHELL").ok().as_deref() {
            Some("fish") => println!(
                "set -gx ADESH_HOME '{}';\nset -gx ADESH_TOOLCHAIN '{}/toolchain/llvm';\nfish_add_path '{}';",
                home, home, bin
            ),
            _ if cfg!(windows) => println!(
                "$env:ADESH_HOME = '{}'\n$env:ADESH_TOOLCHAIN = '{}\\toolchain\\llvm'\n$env:Path = '{};' + $env:Path",
                home, home, bin
            ),
            _ => println!(
                "export ADESH_HOME='{}'\nexport ADESH_TOOLCHAIN='{}/toolchain/llvm'\nexport PATH='{}:$PATH'",
                home, home, bin
            ),
        }
        return;
    }
    let cyan = "\x1b[36m";
    let green = "\x1b[32m";
    let yellow = "\x1b[33m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    println!("{}AdeshLang Environment Info{}", bold, reset);
    println!("────────────────────────────────────────────");

    let host_triple = if cfg!(target_arch = "wasm32") {
        "wasm32-unknown-unknown".to_string()
    } else {
        #[cfg(not(target_arch = "wasm32"))]
        {
            target_lexicon::Triple::host().to_string()
        }
        #[cfg(target_arch = "wasm32")]
        {
            "wasm32-unknown-unknown".to_string()
        }
    };

    // Version
    println!(
        "  {:18} {}v{} ({}){}",
        "AdeshLang Version:",
        green,
        env!("CARGO_PKG_VERSION"),
        host_triple,
        reset
    );

    // Installation home (ADESH_HOME, with legacy ADESHLANG_HOME alias)
    let home = installation_home();

    if let Some(h) = home {
        println!("  {:18} {}", "Home Directory:", h.display());
    } else {
        println!(
            "  {:18} {}Not explicitly set (using executable-relative fallback){}",
            "Home Directory:", yellow, reset
        );
    }

    // Toolchain
    let toolchain = bundled_root().or_else(|| env::var_os("ADESH_TOOLCHAIN").map(PathBuf::from));

    if let Some(tc) = toolchain {
        println!("  {:18} {}", "Toolchain Directory:", tc.display());
    } else {
        println!(
            "  {:18} {}Bundled / System LLVM{}",
            "Toolchain Directory:", cyan, reset
        );
    }

    // Standard library
    let std_dir = env_var_alias("ADESH_STD", "ADESHLANG_STD")
        .map(PathBuf::from)
        .or_else(|| {
            env_var_alias("ADESH_HOME", "ADESHLANG_HOME")
                .map(|h| PathBuf::from(h).join("std"))
                .or_else(|| env::current_dir().ok().map(|cd| cd.join("lib").join("std")))
        });

    if let Some(std_p) = std_dir {
        println!("  {:18} {}", "Standard Library:", std_p.display());
    } else {
        println!(
            "  {:18} {}Default (built-in/relative){}",
            "Standard Library:", cyan, reset
        );
    }

    // Packages directory
    let packages_dir = env_var_alias("ADESH_PACKAGES", "ADESHLANG_PACKAGES")
        .map(PathBuf::from)
        .or_else(|| {
            env::var("LOCALAPPDATA")
                .ok()
                .map(|d| PathBuf::from(d).join("AdeshLang").join("packages"))
        });

    if let Some(pkg_p) = packages_dir {
        println!("  {:18} {}", "Package Directory:", pkg_p.display());
    }

    // Cache directory
    let cache_dir = env_var_alias("ADESH_CACHE", "ADESHLANG_CACHE")
        .map(PathBuf::from)
        .or_else(|| {
            env::var("LOCALAPPDATA")
                .ok()
                .map(|d| PathBuf::from(d).join("AdeshLang").join("cache"))
        });

    if let Some(c_p) = cache_dir {
        println!("  {:18} {}", "Cache Directory:", c_p.display());
    }

    // Host & Target
    #[cfg(not(target_arch = "wasm32"))]
    println!("  {:18} {}", "Host Triple:", target_lexicon::Triple::host());
    #[cfg(target_arch = "wasm32")]
    println!("  {:18} {}", "Host Triple:", "wasm32-unknown-unknown");
    println!("  {:18} {}", "Operating System:", env::consts::OS);
    println!("  {:18} {}", "Architecture:", env::consts::ARCH);
    println!();
}
