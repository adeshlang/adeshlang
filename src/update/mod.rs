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
