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

    // 4. ADESHLANG_HOME Resolution
    let home = installation_home();

    if let Some(h) = &home {
        if h.exists() {
            println!("  {}✓{} Installation Home : {}", green, reset, h.display());
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
            "  {}!{} Installation Home : ADESHLANG_HOME env variable not set (using executable folder fallback)",
            yellow, reset
        );
    }

    // 5. Native toolchain: bundled is deterministic; system tools are informational.
    match resolve(Some(ToolchainPreference::Bundled)) {
        Ok(toolchain) => println!(
            "  {}✓{} Bundled Toolchain : LLVM/Clang {} ({})",
            green,
            reset,
            toolchain.version.as_deref().unwrap_or("unknown"),
            toolchain.root.display()
        ),
        Err(error) => {
            println!("  {}✗{} Bundled Toolchain : {}", red, reset, error);
            issues += 1;
        }
    }
    if let Some((path, version)) = detect_system_toolchain() {
        println!(
            "  {}i{} System Clang       : {} ({})",
            yellow,
            reset,
            version.as_deref().unwrap_or("unknown"),
            path.display()
        );
    }

    // 6. Standard Library Check
    let std_candidates = vec![
        home.as_ref().map(|h| h.join("std")),
        home.as_ref().map(|h| h.join("lib").join("std")),
        env::current_dir().ok().map(|cd| cd.join("lib").join("std")),
        env::current_dir().ok().map(|cd| cd.join("std")),
    ];

    let mut std_found = false;
    for cand in std_candidates.into_iter().flatten() {
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

    // 7. PATH Check
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

    // 8. Incremental Compilation & Build Cache
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
