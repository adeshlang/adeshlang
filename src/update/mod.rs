//! Incremental Component Update System for AdeshLang.
//!
//! Provides:
//! - Component-level delta updates (compiler, stdlib, runtime, AI models)
//! - Atomic versioned layout (`versions/<version>/` + `current.json`)
//! - Cryptographic SHA-256 verification
//! - Safe zero-downtime Windows binary replacement
//! - Instant rollback (`adesh rollback`)

pub mod checker;
pub mod downloader;
pub mod installer;
pub mod manifest;
pub mod rollback;
pub mod verifier;

use colored::Colorize;
use manifest::{CurrentInstallation, UpdateChannel};
use std::path::PathBuf;
use std::process::Command;

/// Locate the active Adesh installation root
pub fn active_installation_home() -> PathBuf {
    crate::toolchain::resolver::installation_home().unwrap_or_else(|| PathBuf::from("."))
}

/// Execute `adesh update` command
pub fn execute_update_command(args: &[String]) {
    let mut check_only = false;
    let mut channel = None;
    let mut custom_url = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--check" | "-c" => check_only = true,
            "--channel" if i + 1 < args.len() => {
                i += 1;
                channel = UpdateChannel::from_str_name(&args[i]);
            }
            "--url" if i + 1 < args.len() => {
                i += 1;
                custom_url = Some(args[i].clone());
            }
            _ => {}
        }
        i += 1;
    }

    if let Some(url) = custom_url {
        unsafe {
            std::env::set_var("ADESH_UPDATE_URL", url);
        }
    }

    let home = active_installation_home();
    let current_inst = CurrentInstallation::load_or_init(&home);

    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );
    println!("{}", "  AdeshLang Update Manager".bold().green());
    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );
    println!("  Current version : {}", current_inst.version.yellow());
    println!(
        "  Channel         : {}",
        current_inst.channel.as_str().cyan()
    );
    println!("  Installation    : {}", home.display());
    println!();

    print!("==> Checking for available updates... ");
    let check_res = match checker::check_for_updates(&current_inst, channel) {
        Ok(res) => {
            println!("{}", "OK".bold().green());
            res
        }
        Err(e) => {
            println!("{}", "FAILED".bold().red());
            eprintln!("{}: {e}", "Error checking update".red());
            return;
        }
    };

    if !check_res.is_update_available {
        println!("{}", "✓ AdeshLang is already up to date.".bold().green());
        return;
    }

    println!(
        "  Latest version  : {}",
        check_res.latest_version.bold().green()
    );
    println!("  Released at     : {}", check_res.released_at);
    println!();
    println!("{}", "Updates available for components:".bold());

    for (comp_name, comp_info) in &check_res.components_to_update {
        let size_mb = (comp_info.size as f64) / (1024.0 * 1024.0);
        println!(
            "  • {:<12} -> {:<8} ({:.2} MB)",
            comp_name.cyan(),
            comp_info.version.green(),
            size_mb
        );
    }

    for comp_name in &check_res.components_unchanged {
        println!("  • {:<12} [Up to date, skipped]", comp_name.dimmed());
    }

    let total_mb = (check_res.total_download_size as f64) / (1024.0 * 1024.0);
    println!();
    println!("Total download size: {:.2} MB", total_mb);

    if check_only {
        println!();
        println!("Run {} to install updates.", "'adesh update'".bold().cyan());
        return;
    }

    println!();
    println!("{}", "==> Applying incremental update...".cyan());

    let apply_res = installer::apply_update(&home, &check_res, |msg, cur, total| {
        println!("  [{cur}/{total}] {msg}...");
    });

    match apply_res {
        Ok(()) => {
            println!();
            println!(
                "{} AdeshLang updated successfully to v{}!",
                "✓".bold().green(),
                check_res.latest_version.bold().green()
            );
            println!();

            // Run adesh doctor to verify installation health
            let adesh_bin = home.join("bin").join("adesh.exe");
            if adesh_bin.exists() {
                println!("{}", "==> Verifying system health (adesh doctor)...".cyan());
                let _ = Command::new(&adesh_bin).arg("doctor").status();
            }
        }
        Err(e) => {
            eprintln!();
            eprintln!("{}: {e}", "Update installation failed".bold().red());
            eprintln!("Your previous installation remains safe. Run 'adesh rollback' if needed.");
        }
    }
}

/// Execute `adesh rollback` command
pub fn execute_rollback_command(_args: &[String]) {
    let home = active_installation_home();

    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );
    println!("{}", "  AdeshLang Rollback Manager".bold().yellow());
    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );
    println!("  Installation Path: {}", home.display());
    println!();

    print!("==> Rolling back to previous version... ");
    match rollback::rollback_installation(&home) {
        Ok(restored_version) => {
            println!("{}", "OK".bold().green());
            println!(
                "{} Successfully rolled back to version {}",
                "✓".bold().green(),
                restored_version.bold().yellow()
            );
            println!();

            let adesh_bin = home.join("bin").join("adesh.exe");
            if adesh_bin.exists() {
                let _ = Command::new(&adesh_bin).arg("doctor").status();
            }
        }
        Err(e) => {
            println!("{}", "FAILED".bold().red());
            eprintln!("{}: {e}", "Rollback failed".bold().red());
        }
    }
}

/// Execute `adesh ai update` for heavy model files
pub fn execute_ai_update_command(_args: &[String]) {
    let home = active_installation_home();
    let current_inst = CurrentInstallation::load_or_init(&home);

    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );
    println!("{}", "  AdeshLang AI Model Updater".bold().magenta());
    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );

    let manifest = match checker::fetch_remote_manifest(current_inst.channel) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{}: {e}", "Failed to check AI model updates".red());
            return;
        }
    };

    if let Some(ai_comp) = manifest.components.get("ai_model") {
        let ai_dir = home.join("ai").join("models");
        let _ = std::fs::create_dir_all(&ai_dir);
        let dest = ai_dir.join("adesh-coder-0.5b-q4_0.gguf");

        let size_mb = (ai_comp.size as f64) / (1024.0 * 1024.0);
        println!(
            "  Target Model : {} (v{})",
            "adesh-coder".cyan(),
            ai_comp.version.green()
        );
        println!("  Size         : {:.2} MB", size_mb);
        println!("  Destination  : {}", dest.display());
        println!();

        print!("==> Downloading AI model weights... ");
        match downloader::download_file(&ai_comp.url, &dest, Some(ai_comp.size)) {
            Ok(()) => {
                println!("{}", "OK".bold().green());
                println!("==> Verifying model checksum... ");
                match verifier::verify_checksum(&dest, &ai_comp.sha256) {
                    Ok(true) => {
                        println!("{}", "✓ AI model updated and verified!".bold().green());
                    }
                    _ => {
                        eprintln!("{}", "Verification failed: Checksum mismatch!".bold().red());
                    }
                }
            }
            Err(e) => {
                println!("{}", "FAILED".bold().red());
                eprintln!("{}: {e}", "Download failed".red());
            }
        }
    } else {
        println!("No AI model updates available in release manifest.");
    }
}

/// Standalone AI Model Downloader and Setup
#[derive(Clone, Copy)]
pub struct ModelArtifact {
    pub filename: &'static str,
    pub quantization: &'static str,
    pub size_mb: f64,
    pub size_bytes: u64,
    pub sha256: &'static str,
    pub url: &'static str,
}

pub const DEFAULT_Q4_MODEL: ModelArtifact = ModelArtifact {
    filename: "adesh-coder-0.5b-q4_0.gguf",
    quantization: "Q4_0",
    size_mb: 274.98,
    size_bytes: 288332608,
    sha256: "fdd91c75c948e167033ae5dc693212816df0c1220acc02659f189a71c9e16cbe",
    url: "https://huggingface.co/adeshlang/adesh-coder-0.5b/resolve/main/adesh-coder-0.5b-q4_0.gguf",
};

pub const Q8_MODEL: ModelArtifact = ModelArtifact {
    filename: "adesh-coder-0.5b-q8_0.gguf",
    quantization: "Q8_0",
    size_mb: 510.51,
    size_bytes: 535313216,
    sha256: "a42a6ed8a2676ce4efa67f79cb081f8c94f1ee3d861068821f461e237220374e",
    url: "https://huggingface.co/adeshlang/adesh-coder-0.5b/resolve/main/adesh-coder-0.5b-q8_0.gguf",
};

pub const F16_MODEL: ModelArtifact = ModelArtifact {
    filename: "adesh-coder-0.5b-f16.gguf",
    quantization: "F16",
    size_mb: 948.1,
    size_bytes: 994156352,
    sha256: "b4db60791d984dba3641cd5055c85f2a68869f3701be2c2f651a899a8c3582a9",
    url: "https://huggingface.co/adeshlang/adesh-coder-0.5b/resolve/main/adesh-coder-0.5b-f16.gguf",
};

/// Execute `adesh ai setup` or `adl ai setup` to fetch and verify the AI model post-install
pub fn execute_ai_setup_command(args: &[String]) {
    use std::io::Write;

    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );
    println!(
        "{}",
        "  AdeshLang AI Model Downloader & Setup".bold().magenta()
    );
    println!(
        "{}",
        "──────────────────────────────────────────────────────".cyan()
    );

    let mut artifact = DEFAULT_Q4_MODEL;
    let mut custom_dest: Option<PathBuf> = None;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--q8" | "--q8_0" => artifact = Q8_MODEL,
            "--f16" => artifact = F16_MODEL,
            "--q4" | "--q4_0" => artifact = DEFAULT_Q4_MODEL,
            "--quantization" | "-q" => {
                if i + 1 < args.len() {
                    i += 1;
                    match args[i].to_uppercase().as_str() {
                        "Q8" | "Q8_0" => artifact = Q8_MODEL,
                        "F16" | "FP16" => artifact = F16_MODEL,
                        _ => artifact = DEFAULT_Q4_MODEL,
                    }
                }
            }
            "--dest" | "--target-dir" | "--dir" => {
                if i + 1 < args.len() {
                    i += 1;
                    custom_dest = Some(PathBuf::from(&args[i]));
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Determine target directory:
    // 1. Explicit --dest
    // 2. ADESH_HOME/ai/models if set
    // 3. Executable's parent's ai/models or bin/
    // 4. Fallback ~/.adesh/models
    let dest_dir = if let Some(d) = custom_dest {
        d
    } else if let Ok(home) =
        std::env::var("ADESH_HOME").or_else(|_| std::env::var("ADESHLANG_HOME"))
    {
        if !home.is_empty() {
            PathBuf::from(home).join("ai").join("models")
        } else {
            fallback_model_dir()
        }
    } else if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            if let Some(grandparent) = parent.parent() {
                let candidate = grandparent.join("ai").join("models");
                if candidate.exists() || grandparent.join("bin").exists() {
                    candidate
                } else {
                    parent.join("ai").join("models")
                }
            } else {
                parent.join("ai").join("models")
            }
        } else {
            fallback_model_dir()
        }
    } else {
        fallback_model_dir()
    };

    let _ = std::fs::create_dir_all(&dest_dir);
    let dest_file = dest_dir.join(artifact.filename);

    println!(
        "  Model Variant : {} ({})",
        artifact.filename.cyan(),
        artifact.quantization.green()
    );
    println!("  Size          : {:.2} MB", artifact.size_mb);
    println!("  Destination   : {}", dest_file.display());
    println!("  Source URL    : {}", artifact.url);
    println!();

    if dest_file.exists() {
        print!("==> Existing model file found, verifying SHA-256... ");
        let _ = std::io::stdout().flush();
        if let Ok(true) = verifier::verify_checksum(&dest_file, artifact.sha256) {
            println!("{}", "VALID".bold().green());
            println!(
                "\n{}",
                "✓ AdeshLang AI model is already installed and verified!"
                    .bold()
                    .green()
            );
            return;
        } else {
            println!("{}", "Corrupted or outdated. Re-downloading...".yellow());
        }
    }

    print!("==> Downloading weights from Hugging Face... ");
    let _ = std::io::stdout().flush();
    match downloader::download_file(artifact.url, &dest_file, Some(artifact.size_bytes)) {
        Ok(()) => {
            println!("{}", "OK".bold().green());
            print!("==> Verifying SHA-256 checksum... ");
            let _ = std::io::stdout().flush();
            match verifier::verify_checksum(&dest_file, artifact.sha256) {
                Ok(true) => {
                    println!("{}", "PASS".bold().green());
                    println!(
                        "\n{}",
                        "✓ AdeshLang AI model installed successfully!"
                            .bold()
                            .green()
                    );
                    println!("You can now run commands like:");
                    println!("  adesh ai generate \"Create a binary search function\"");
                    println!("  adesh ai explain file.adesh");
                    println!("  adesh ai fix broken.adesh");
                    println!("  adesh ai chat");
                }
                _ => {
                    eprintln!("{}", "FAILED (Checksum mismatch)".bold().red());
                    eprintln!("Please re-run `adesh ai setup` to retry.");
                }
            }
        }
        Err(e) => {
            println!("{}", "FAILED".bold().red());
            eprintln!("{}: {e}", "Download error".red());
            eprintln!("\nAlternative: Download manually from:");
            eprintln!("  {}", artifact.url);
            eprintln!("And place in: {}", dest_dir.display());
        }
    }
}

fn fallback_model_dir() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".adesh").join("models")
}
