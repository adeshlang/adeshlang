//! Rollback mechanism for returning to the previous installation state.

use super::installer::sync_snapshot_to_active;
use super::manifest::{CurrentInstallation, chrono_like_now};
use std::path::Path;

/// Roll back to the previously installed version
pub fn rollback_installation(home: &Path) -> Result<String, String> {
    let mut current_inst = CurrentInstallation::load_or_init(home);
    let prev_version = match &current_inst.previous_version {
        Some(v) if !v.trim().is_empty() => v.clone(),
        _ => {
            // Check backup/ directory fallback
            let backup_dir = home.join("backup");
            if backup_dir.exists() && backup_dir.join("bin").exists() {
                "previous_backup".to_string()
            } else {
                return Err("No previous version found to roll back to.".to_string());
            }
        }
    };

    let source_dir = if prev_version == "previous_backup" {
        home.join("backup")
    } else {
        let ver_dir = home.join("versions").join(&prev_version);
        if ver_dir.exists() {
            ver_dir
        } else {
            home.join("backup")
        }
    };

    if !source_dir.exists() {
        return Err(format!(
            "Rollback source directory '{}' does not exist.",
            source_dir.display()
        ));
    }

    // Sync backup/version snapshot into active installation
    sync_snapshot_to_active(&source_dir, home)?;

    let rolled_back_from = current_inst.version.clone();
    current_inst.version = if prev_version == "previous_backup" {
        "restored".to_string()
    } else {
        prev_version.clone()
    };
    current_inst.previous_version = Some(rolled_back_from);
    current_inst.last_checked_at = Some(chrono_like_now());
    let _ = current_inst.save(home);

    Ok(prev_version)
}
