//! Update checker and component delta resolution.

use super::downloader::fetch_text;
use super::manifest::{ComponentInfo, CurrentInstallation, ReleaseManifest, UpdateChannel};
use std::collections::BTreeMap;
use std::env;

pub const DEFAULT_UPDATE_API_URL: &str = "https://raw.githubusercontent.com/adeshlang/adeshlang/main/installer/manifests/release-stable.json";

/// Result of checking for available updates
#[derive(Debug, Clone)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub channel: UpdateChannel,
    pub released_at: String,
    pub is_update_available: bool,
    pub components_to_update: BTreeMap<String, ComponentInfo>,
    pub components_unchanged: Vec<String>,
    pub total_download_size: u64,
    pub manifest: ReleaseManifest,
}

/// Resolve the active manifest endpoint URL
pub fn resolve_manifest_url(channel: UpdateChannel) -> String {
    if let Ok(custom_url) = env::var("ADESH_UPDATE_URL") {
        if !custom_url.trim().is_empty() {
            return custom_url;
        }
    }

    let repo = env::var("ADESH_REPO").unwrap_or_else(|_| "adeshlang/adeshlang".to_string());
    let branch = env::var("ADESH_UPDATE_BRANCH").unwrap_or_else(|_| "main".to_string());
    match channel {
        UpdateChannel::Stable => {
            format!(
                "https://raw.githubusercontent.com/{repo}/{branch}/installer/manifests/release-stable.json"
            )
        }
        UpdateChannel::Beta => {
            format!(
                "https://raw.githubusercontent.com/{repo}/{branch}/installer/manifests/release-beta.json"
            )
        }
        UpdateChannel::Nightly => {
            format!(
                "https://raw.githubusercontent.com/{repo}/{branch}/installer/manifests/release-nightly.json"
            )
        }
    }
}

/// Fetch the remote release manifest
pub fn fetch_remote_manifest(channel: UpdateChannel) -> Result<ReleaseManifest, String> {
    let url = resolve_manifest_url(channel);
    let json_content = fetch_text(&url)?;
    ReleaseManifest::from_json(&json_content)
}

/// Compare currently installed state with the remote release manifest
pub fn check_for_updates(
    current: &CurrentInstallation,
    channel: Option<UpdateChannel>,
) -> Result<UpdateCheckResult, String> {
    let target_channel = channel.unwrap_or(current.channel);
    let remote_manifest = fetch_remote_manifest(target_channel)?;

    let is_newer_version = is_version_newer(&current.version, &remote_manifest.version);
    let mut components_to_update = BTreeMap::new();
    let mut components_unchanged = Vec::new();
    let mut total_download_size = 0u64;

    for (comp_name, comp_info) in &remote_manifest.components {
        // AI model is updated independently via `adesh ai update`
        if comp_name == "ai_model" {
            continue;
        }

        let needs_update = if let Some(installed) = current.components.get(comp_name) {
            installed.version != comp_info.version
                || !installed.sha256.eq_ignore_ascii_case(&comp_info.sha256)
        } else {
            // Component not installed or version changed
            is_newer_version
        };

        if needs_update || is_newer_version {
            components_to_update.insert(comp_name.clone(), comp_info.clone());
            total_download_size += comp_info.size;
        } else {
            components_unchanged.push(comp_name.clone());
        }
    }

    let is_update_available = is_newer_version || !components_to_update.is_empty();

    Ok(UpdateCheckResult {
        current_version: current.version.clone(),
        latest_version: remote_manifest.version.clone(),
        channel: target_channel,
        released_at: remote_manifest.released_at.clone(),
        is_update_available,
        components_to_update,
        components_unchanged,
        total_download_size,
        manifest: remote_manifest,
    })
}

/// Simple semver comparator: returns true if candidate > current
pub fn is_version_newer(current: &str, candidate: &str) -> bool {
    let parse_parts = |v: &str| -> Vec<u64> {
        let clean = v.trim_start_matches('v').split('-').next().unwrap_or(v);
        clean
            .split('.')
            .map(|p| p.parse::<u64>().unwrap_or(0))
            .collect()
    };

    let c_parts = parse_parts(current);
    let n_parts = parse_parts(candidate);

    for (n, c) in n_parts.iter().zip(c_parts.iter()) {
        if n > c {
            return true;
        }
        if n < c {
            return false;
        }
    }

    n_parts.len() > c_parts.len()
}
