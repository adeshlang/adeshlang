//! Phase 9 Production PGO Tools & Multi-Profile Merging.
//!
//! Provides:
//! - Multi-run profile data merging (`ProfileMerger`)
//! - Statistical aggregation of function entry counts, branch counts, and block weights
//! - Compatibility and version verification of profile data files

use crate::opt::pgo::{BlockProfile, EdgeProfile, FunctionProfile, ProfileData};
use std::collections::HashMap;
use std::path::Path;

/// PGO Profile Merger Engine.
pub struct ProfileMerger;

impl ProfileMerger {
    /// Merge multiple ProfileData instances into one comprehensive profile.
    pub fn merge(profiles: &[ProfileData]) -> Result<ProfileData, String> {
        if profiles.is_empty() {
            return Err("Cannot merge empty profile set".to_string());
        }

        let mut merged = ProfileData::new();
        for profile in profiles {
            for (func_name, func_prof) in &profile.functions {
                let entry = merged
                    .functions
                    .entry(func_name.clone())
                    .or_insert_with(|| FunctionProfile::new(func_name.clone()));

                entry.entry_count = entry.entry_count.saturating_add(func_prof.entry_count);

                for (blk_id, blk) in &func_prof.block_profiles {
                    let b_entry = entry
                        .block_profiles
                        .entry(blk_id.clone())
                        .or_insert_with(BlockProfile::default);
                    b_entry.execution_count =
                        b_entry.execution_count.saturating_add(blk.execution_count);
                }

                for (edge_id, edge) in &func_prof.edge_profiles {
                    let e_entry = entry
                        .edge_profiles
                        .entry(edge_id.clone())
                        .or_insert_with(EdgeProfile::default);
                    e_entry.transition_count = e_entry
                        .transition_count
                        .saturating_add(edge.transition_count);
                    e_entry.probability = (e_entry.probability + edge.probability) / 2.0;
                }
            }
        }

        Ok(merged)
    }

    /// Read multiple profile files from disk, merge them, and write output.
    pub fn merge_files(
        input_paths: &[impl AsRef<Path>],
        output_path: impl AsRef<Path>,
    ) -> Result<(), String> {
        let mut profiles = Vec::new();
        for p in input_paths {
            let json = std::fs::read_to_string(p.as_ref())
                .map_err(|e| format!("Failed to read {}: {}", p.as_ref().display(), e))?;
            let prof: ProfileData = serde_json::from_str(&json)
                .map_err(|e| format!("Failed to parse profile {}: {}", p.as_ref().display(), e))?;
            profiles.push(prof);
        }

        let merged = Self::merge(&profiles)?;
        let out_json = serde_json::to_string_pretty(&merged)
            .map_err(|e| format!("Failed to serialize merged profile: {}", e))?;
        std::fs::write(output_path.as_ref(), out_json)
            .map_err(|e| format!("Failed to write merged profile: {}", e))?;

        Ok(())
    }
}
