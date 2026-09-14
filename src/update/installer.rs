//! Atomic installation engine for component-level updates.

use super::checker::UpdateCheckResult;
use super::downloader::download_file;
use super::manifest::{ComponentInfo, CurrentInstallation, InstalledComponent, chrono_like_now};
use super::verifier::verify_checksum;
use crate::toolchain::archive::extract;
use std::fs;
use std::path::{Path, PathBuf};

/// Perform the full incremental update workflow
pub fn apply_update(
    home: &Path,
    check_result: &UpdateCheckResult,
    progress_callback: impl Fn(&str, usize, usize),
) -> Result<(), String> {
    let total_components = check_result.components_to_update.len();
    if total_components == 0 {
        return Ok(());
    }

    let temp_staging_dir = home
        .join(".update_staging")
        .join(&check_result.latest_version);
    let versions_dir = home.join("versions");
    let target_version_dir = versions_dir.join(&check_result.latest_version);
    let backup_dir = home.join("backup");

    // Clean any prior failed staging
    if temp_staging_dir.exists() {
        let _ = fs::remove_dir_all(&temp_staging_dir);
    }
    fs::create_dir_all(&temp_staging_dir).map_err(|e| {
        format!(
            "Failed to create staging directory '{}': {e}",
            temp_staging_dir.display()
        )
    })?;

    // 1. Download and verify components in staging
    let mut downloaded_archives: Vec<(String, ComponentInfo, PathBuf)> = Vec::new();
    let mut current_idx = 0;

    for (comp_name, comp_info) in &check_result.components_to_update {
        current_idx += 1;
        progress_callback(
            &format!("Downloading {comp_name} ({})", comp_info.version),
            current_idx,
            total_components,
        );

        let filename = format!("{comp_name}-{}.zip", comp_info.version);
        let archive_dest = temp_staging_dir.join(&filename);

        download_file(&comp_info.url, &archive_dest, Some(comp_info.size))
            .map_err(|e| format!("Failed to download component '{comp_name}': {e}"))?;

        progress_callback(
            &format!("Verifying {comp_name} checksum"),
            current_idx,
            total_components,
        );

        let is_valid = verify_checksum(&archive_dest, &comp_info.sha256)?;
        if !is_valid {
            return Err(format!(
                "Security verification failed: SHA-256 digest mismatch for component '{comp_name}'"
            ));
        }

        downloaded_archives.push((comp_name.clone(), comp_info.clone(), archive_dest));
    }

    // 2. Unpack into target version snapshot: versions/<latest_version>/
    if target_version_dir.exists() {
        let _ = fs::remove_dir_all(&target_version_dir);
    }
    fs::create_dir_all(&target_version_dir).map_err(|e| {
        format!(
            "Failed to create version directory '{}': {e}",
            target_version_dir.display()
        )
    })?;

    // First clone current active files into new version snapshot (so unchanged components persist)
    clone_active_into_snapshot(home, &target_version_dir)?;

    for (comp_name, _comp_info, archive_path) in &downloaded_archives {
        let comp_dest = match comp_name.as_str() {
            "compiler" => target_version_dir.join("bin"),
            "stdlib" => target_version_dir.join("std"),
            "runtime" => target_version_dir.join("lib"),
            "tools" => target_version_dir.join("tools"),
            _ => target_version_dir.join(comp_name),
        };

        fs::create_dir_all(&comp_dest)
            .map_err(|e| format!("Failed to create component destination '{comp_dest:?}': {e}"))?;

        let _ = extract(archive_path, "zip", &comp_dest)?;
    }

    // 3. Backup previous version for instant rollback
    if backup_dir.exists() {
        let _ = fs::remove_dir_all(&backup_dir);
    }
    fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {e}"))?;
    let _ = clone_active_into_snapshot(home, &backup_dir);

    // 4. Atomically sync active directories: bin/, lib/, std/
    sync_snapshot_to_active(&target_version_dir, home)?;

    // 5. Update current.json
    let mut current_inst = CurrentInstallation::load_or_init(home);
    current_inst.previous_version = Some(current_inst.version.clone());
    current_inst.version = check_result.latest_version.clone();
    current_inst.channel = check_result.channel;
    current_inst.last_checked_at = Some(chrono_like_now());

    for (comp_name, comp_info, _) in downloaded_archives {
        current_inst.components.insert(
            comp_name,
            InstalledComponent {
                version: comp_info.version,
                sha256: comp_info.sha256,
                updated_at: chrono_like_now(),
            },
        );
    }

    current_inst.save(home)?;

    // Clean up temporary staging
    let _ = fs::remove_dir_all(&temp_staging_dir);

    Ok(())
}

fn clone_active_into_snapshot(home: &Path, snapshot: &Path) -> Result<(), String> {
    for folder in &["bin", "lib", "std", "config"] {
        let src = home.join(folder);
        let dst = snapshot.join(folder);
        if src.exists() {
            copy_dir_all(&src, &dst)?;
        }
    }
    Ok(())
}

pub fn sync_snapshot_to_active(snapshot: &Path, home: &Path) -> Result<(), String> {
    for folder in &["bin", "lib", "std", "config"] {
        let src = snapshot.join(folder);
        let dst = home.join(folder);
        if src.exists() {
            copy_dir_all_safe_windows(&src, &dst)?;
        }
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create dir '{dst:?}': {e}"))?;
    for entry in fs::read_dir(src).map_err(|e| format!("Failed to read dir '{src:?}': {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_all(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)
                .map_err(|e| format!("Failed to copy '{src_path:?}' to '{dst_path:?}': {e}"))?;
        }
    }
    Ok(())
}

fn copy_dir_all_safe_windows(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("Failed to create dir '{dst:?}': {e}"))?;
    for entry in fs::read_dir(src).map_err(|e| format!("Failed to read dir '{src:?}': {e}"))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_all_safe_windows(&src_path, &dst_path)?;
        } else {
            // Windows atomic replacement for active .exe
            if dst_path.exists()
                && dst_path
                    .extension()
                    .is_some_and(|ext| ext == "exe" || ext == "dll")
            {
                let old_backup = dst_path.with_extension("exe.old");
                if old_backup.exists() {
                    let _ = fs::remove_file(&old_backup);
                }
                let _ = fs::rename(&dst_path, &old_backup);
            }

            fs::copy(&src_path, &dst_path).map_err(|e| {
                format!("Failed to copy file '{src_path:?}' to '{dst_path:?}': {e}")
            })?;
        }
    }
    Ok(())
}
