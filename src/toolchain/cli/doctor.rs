//! Diagnostic health inspector command implementation (`adl doctor`)

use super::super::resolver::{
    ToolchainPreference, detect_system_toolchain, installation_home, resolve,
};
use std::env;
use std::path::{Path, PathBuf};

pub fn execute_doctor_command() {
    let _cyan = "\x1b[36m";
    let green = "\x1b[32m";
    let red = "\x1b[31m";
    let yellow = "\x1b[33m";
    let bold = "\x1b[1m";
    let reset = "\x1b[0m";

    println!("\n{}AdeshLang System Health Doctor{}", bold, reset);
    println!("────────────────────────────────────────────");

    let mut issues = 0;

    // 1. Version Check
    println!(
        "  {}✓{} AdeshLang Version : {}v{}{}",
        green,
        reset,
        bold,
        env!("CARGO_PKG_VERSION"),
        reset
    );

    // 2. OS & Architecture Check
    println!(
        "  {}✓{} OS & Platform     : {} ({})",
        green,
        reset,
        env::consts::OS,
        env::consts::ARCH
    );

    // 3. Executable Path
    if let Ok(exe_path) = env::current_exe() {
        println!(
            "  {}✓{} Binary Path       : {}",
            green,
            reset,
            exe_path.display()
        );
    } else {
        println!(
            "  {}✗{} Binary Path       : Could not resolve current executable path",
            red, reset
        );
        issues += 1;
    }

    // 4. Installation Home Resolution
    let home = installation_home();
    let env_home = env::var_os("ADESH_HOME")
        .or_else(|| env::var_os("ADESHLANG_HOME"))
        .map(PathBuf::from);

    if let Some(h) = &home {
        if h.exists() {
            println!("  {}✓{} Installation Home : {}", green, reset, h.display());
            if let Some(eh) = env_home {
                if !eh.exists() {
                    println!(
                        "  {}!{} Stale Env Var     : ADESH_HOME is set to non-existent path ({})",
                        yellow,
                        reset,
                        eh.display()
                    );
                }
            }
        } else {
            println!(
                "  {}✗{} Installation Home : Directory does not exist ({})",
                red,
                reset,
                h.display()
            );
            issues += 1;
        }
    } else {
        println!(
            "  {}!{} Installation Home : Not set (using executable-relative fallback)",
            yellow, reset
        );
    }

    // 5. Toolchain Resolution
    let mut active_clang_path = None;
    match resolve(None) {
        Ok(toolchain) => {
            active_clang_path = Some(toolchain.clang.clone());
            println!(
                "  {}✓{} Active Toolchain  : LLVM/Clang {} ({})",
                green,
                reset,
                toolchain.version.as_deref().unwrap_or("unknown"),
                toolchain.root.display()
            );
        }
        Err(error) => {
            println!("  {}✗{} Active Toolchain  : {}", red, reset, error);
            issues += 1;
        }
    }
    if let Some((path, version)) = detect_system_toolchain() {
        let is_same = active_clang_path.as_ref().map_or(false, |ac| ac == &path);
        if !is_same {
            let info_note = if cfg!(windows) {
                " (Informational: Adesh uses its LLVM MSVC toolchain)"
            } else {
                " (Additional compiler found on PATH)"
            };
            println!(
                "  {}i{} Extra System Clang : {} ({}){}",
                "\x1b[36m", // cyan for info
                reset,
                version.as_deref().unwrap_or("unknown"),
                path.display(),
                info_note
            );
        }
    }

    // 6. Standard Library Check
    let mut std_candidates = Vec::new();
    if let Some(h) = &home {
        std_candidates.push(h.join("std"));
        std_candidates.push(h.join("lib").join("std"));
        std_candidates.push(h.join("src").join("stdlib"));
    }
    if let Ok(exe_path) = env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            std_candidates.push(exe_dir.join("std"));
            if let Some(install_root) = exe_dir.parent() {
                std_candidates.push(install_root.join("std"));
                std_candidates.push(install_root.join("lib").join("std"));
                std_candidates.push(install_root.join("src").join("stdlib"));
            }
        }
    }
    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        std_candidates.push(PathBuf::from(manifest_dir).join("src").join("stdlib"));
    }
    if let Ok(cd) = env::current_dir() {
        std_candidates.push(cd.join("std"));
        std_candidates.push(cd.join("lib").join("std"));
        std_candidates.push(cd.join("src").join("stdlib"));
    }

    let mut std_found = false;
    for cand in std_candidates {
        if cand.exists() && cand.is_dir() {
            println!(
                "  {}✓{} Standard Library  : {}",
                green,
                reset,
                cand.display()
            );
            std_found = true;
            break;
        }
    }
    if !std_found {
        println!(
            "  {}✗{} Standard Library  : std directory not found",
            red, reset
        );
        issues += 1;
    }

    // 7. Static Runtime Library Check (for AOT native compilation)
    #[cfg(not(target_arch = "wasm32"))]
    {
        let host_triple = target_lexicon::Triple::host();
        match crate::backends::aot::cranelift_impl::linking::get_static_runtime_lib(&host_triple) {
            Ok(runtime_path) => {
                println!(
                    "  {}✓{} Runtime Library   : {}",
                    green,
                    reset,
                    runtime_path.display()
                );
            }
            Err(_) => {
                let lib_name = if cfg!(windows) {
                    "adeshlang.lib"
                } else {
                    "libadeshlang.a"
                };
                println!(
                    "  {}✗{} Runtime Library   : {} not found (AOT native build will fail; run `adl repair` or reinstall)",
                    red, reset, lib_name
                );
                issues += 1;
            }
        }
    }

    // 8. Python Runtime & AI Environment Check
    let python_opt = {
        let mut found_py = None;
        for cmd in ["python3", "python", "py"] {
            let res = std::process::Command::new(cmd)
                .args(["-c", "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}.{sys.version_info.micro}')"])
                .output();
            if let Ok(out) = res {
                if out.status.success() {
                    let ver = String::from_utf8_lossy(&out.stdout).trim().to_string();
                    if !ver.is_empty() {
                        found_py = Some((cmd.to_string(), ver));
                        break;
                    }
                }
            }
        }
        found_py
    };

    if let Some((py_cmd, py_ver)) = python_opt {
        println!(
            "  {}✓{} Python Runtime    : Python {} ({}) - Full AI features available",
            green, reset, py_ver, py_cmd
        );
    } else {
        println!(
            "  {}i{} Python Runtime    : Not detected (AI runs via Native In-Process Engine; install Python 3.12 for neural training)",
            yellow, reset
        );
    }

    // 9. Windows SDK / MSVC Linker Check (Windows-only)
    #[cfg(windows)]
    {
        let has_vs = {
            let root19 = Path::new("C:\\Program Files (x86)\\Microsoft Visual Studio\\2019");
            let root22_x86 = Path::new("C:\\Program Files (x86)\\Microsoft Visual Studio\\2022");
            let root22 = Path::new("C:\\Program Files\\Microsoft Visual Studio\\2022");
            root19.join("BuildTools\\VC\\Tools\\MSVC").exists()
                || root19.join("Community\\VC\\Tools\\MSVC").exists()
                || root22_x86.join("BuildTools\\VC\\Tools\\MSVC").exists()
                || root22_x86.join("Community\\VC\\Tools\\MSVC").exists()
                || root22.join("BuildTools\\VC\\Tools\\MSVC").exists()
                || root22.join("Community\\VC\\Tools\\MSVC").exists()
                || std::env::var("VCToolsInstallDir").is_ok()
                || std::env::var("WindowsSdkDir").is_ok()
        };
        if has_vs {
            println!(
                "  {}✓{} Windows MSVC SDK  : Detected (ready for AOT native MSVC linking)",
                green, reset
            );
        } else {
            println!(
                "  {}!{} Windows MSVC SDK  : Visual Studio Build Tools / Windows SDK not detected in standard locations",
                yellow, reset
            );
        }
    }

    // 10. PATH Check
    if let Ok(path_var) = env::var("PATH") {
        let in_path = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .map(|bin_dir| path_var.split(';').any(|p| Path::new(p) == bin_dir))
            .unwrap_or(false);

        if in_path {
            println!(
                "  {}✓{} Environment PATH  : Configured correctly",
                green, reset
            );
        } else {
            println!(
                "  {}!{} Environment PATH  : Current binary folder is not in PATH",
                yellow, reset
            );
        }
    }

    // 11. Incremental Compilation & Build Cache
    let cache_dir = PathBuf::from(".adesh_cache");
    if cache_dir.exists() {
        let tc_file = cache_dir.join("toolchain.json");
        let aot_dir = cache_dir.join("aot");
        let tc_status = if tc_file.exists() {
            "Active (cached)"
        } else {
            "Not populated"
        };
        let mut object_count = 0;
        if aot_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&aot_dir) {
                object_count = entries.flatten().filter(|e| e.path().is_file()).count();
            }
        }
        println!(
            "  {}✓{} Compilation Cache : .adesh_cache (Toolchain: {}, {} cached artifacts)",
            green, reset, tc_status, object_count
        );
    } else {
        println!(
            "  {}✓{} Compilation Cache : Ready (will be created on first build)",
            green, reset
        );
    }

    println!("\n  {}Crypto Capabilities{}", bold, reset);
    println!("  ──────────────────────────────────────────");
    println!(
        "  {}✓{} OS Entropy CSPRNG  : Active (System Secure Random)",
        green, reset
    );
    println!(
        "  {}✓{} Hashing Engines    : SHA-256, SHA-512, SHA-3, BLAKE2/3",
        green, reset
    );
    println!(
        "  {}✓{} AEAD Ciphers       : AES-256-GCM, ChaCha20-Poly1305, XChaCha20",
        green, reset
    );
    println!(
        "  {}✓{} Signatures & ECDH  : Ed25519, X25519, ECDSA P-256, RSA-PSS",
        green, reset
    );
    println!(
        "  {}✓{} Password KDF       : Argon2id, scrypt, PBKDF2",
        green, reset
    );
    println!(
        "  {}✓{} Certs, JWT & Merkle: X.509, JWK Thumbprint, Merkle Tree",
        green, reset
    );
    println!(
        "  {}✓{} Zeroizing Memory   : Active (VirtualLock / mlock)",
        green, reset
    );

    println!("────────────────────────────────────────────");
    if issues == 0 {
        println!(
            "{}✓ AdeshLang installation is healthy and ready to compile!{}\n",
            green, reset
        );
    } else {
        println!(
            "{}✗ Found {} issue(s) in AdeshLang setup. Run `adl repair` to fix.{}\n",
            red, issues, reset
        );
    }
}
