//! Toolchain manifest: describes the version-pinned third-party toolchains
//! that `adesh toolchain install` downloads during installation.
//!
//! Toolchain artifacts come from their **original upstream sources** (the
//! LLVM project's official GitHub releases), not from Adesh-hosted mirrors.
//! The manifest pins the exact URLs and SHA-256 checksums so installs stay
//! deterministic. The manifest itself is published with AdeshLang releases
//! ([`manifest_url`], derived from [`OFFICIAL_REPO`]).
//!
//! The manifest is consumed from (in priority order):
//!   1. `--manifest <path|url>` on the CLI
//!   2. the `ADESH_TOOLCHAIN_MANIFEST` environment variable
//!   3. `<install-home>/config/toolchain-manifest.json` (bundled with the installer)
//!   4. [`manifest_url()`]
//!
//! Schema (v2):
//! ```json
//! {
//!   "schema": 2,
//!   "adeshVersion": "0.3.0",
//!   "defaultComponents": ["llvm", "mlir"],
//!   "components": {
//!     "llvm": { "version": "18.1.8", "required": true,  "description": "..." },
//!     "mlir": { "version": "18.1.8", "required": false, "description": "..." }
//!   },
//!   "platforms": {
//!     "windows-x86_64": {
//!       "llvm": {
//!         "url": "https://github.com/llvm/llvm-project/releases/.../LLVM-18.1.8-win64.exe",
//!         "sha256": "…", "size": 123, "kind": "installer"
//!       },
//!       "mlir": {
//!         "url": "https://github.com/llvm/llvm-project/releases/.../LLVM-18.1.8-....tar.xz",
//!         "sha256": "…", "size": 123, "format": "tar.xz", "kind": "archive"
//!       }
//!     }
//!   }
//! }
//! ```

use std::collections::BTreeMap;
use std::env;
use std::path::Path;

/// Canonical GitHub repository (`owner/name`) that hosts AdeshLang releases.
///
/// This is the single source of truth for the project's official location.
/// When the repository migrates from a personal account to the official
/// organization, update this constant (and the matching `ADESH_REPO`
/// definitions in the installer scripts and CI workflows) — nothing else.
/// The `ADESH_REPO` environment variable overrides it at runtime for
/// pre-migration testing.
pub const OFFICIAL_REPO: &str = "adeshlang/adeshlang";

/// Stable URL of the toolchain manifest published with each release.
pub fn manifest_url() -> String {
    if let Ok(repo) = env::var("ADESH_REPO") {
        return format!(
            "https://github.com/{repo}/releases/latest/download/toolchain-manifest.json"
        );
    }
    format!("https://github.com/{OFFICIAL_REPO}/releases/latest/download/toolchain-manifest.json")
}

/// Upstream LLVM release tag the toolchain is pinned to. Must match
/// `SUPPORTED_LLVM_MAJOR` in the resolver and the versions in the manifest.
pub const LLVM_RELEASE_TAG: &str = "llvmorg-18.1.8";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolchainManifest {
    /// Must be `2`. Rejects v1 (Windows-only) manifests.
    pub schema: u32,
    pub adesh_version: String,
    /// Components installed when the user does not name any explicitly.
    #[serde(default)]
    pub default_components: Vec<String>,
    pub components: BTreeMap<String, ComponentInfo>,
    pub platforms: BTreeMap<String, BTreeMap<String, ComponentDownload>>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentInfo {
    pub version: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ComponentDownload {
    pub url: String,
    /// Hex-encoded SHA-256 of the artifact. Required; use `--allow-unverified`
    /// on the CLI to install without it (development only).
    #[serde(default)]
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    /// `zip`, `tar.xz`, `tar.gz`, or `tar.zst`. Inferred from the URL when absent.
    #[serde(default)]
    pub format: String,
    /// `archive` (extract) or `installer` (upstream installer executable,
    /// run silently). Defaults to `archive`.
    #[serde(default)]
    pub kind: String,
}

impl ComponentDownload {
    pub fn is_installer(&self) -> bool {
        self.kind.eq_ignore_ascii_case("installer")
    }
}

impl ToolchainManifest {
    /// Validate structure and return the download spec for a platform component.
    pub fn component(&self, platform: &str, component: &str) -> Result<&ComponentDownload, String> {
        let platform_entry = self.platforms.get(platform).ok_or_else(|| {
            format!(
                "Toolchain manifest has no entry for platform `{platform}` (available: {})",
                self.platforms
                    .keys()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })?;
        platform_entry
            .get(component)
            .ok_or_else(|| format!("Platform `{platform}` has no `{component}` component"))
    }

    /// Resolve the components to install for an explicit selection, or the defaults.
    pub fn resolve_components(&self, requested: &[String]) -> Result<Vec<String>, String> {
        if requested.is_empty() {
            if self.default_components.is_empty() {
                return Ok(self.components.keys().cloned().collect());
            }
            return Ok(self.default_components.clone());
        }
        for name in requested {
            if !self.components.contains_key(name) {
                return Err(format!(
                    "Unknown toolchain component `{name}` (available: {})",
                    self.components
                        .keys()
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        Ok(requested.to_vec())
    }
}

/// Canonical platform key used across the manifest, dist scripts, and CI.
///
/// Mirrors `installer/manifest.json`: windows-x86_64, linux-x86_64,
/// linux-aarch64, macos-arm64, macos-x86_64.
pub fn platform_key() -> String {
    platform_key_for(env::consts::OS, env::consts::ARCH)
}

pub fn platform_key_for(os: &str, arch: &str) -> String {
    let arch = match arch {
        "x86_64" | "amd64" => "x86_64",
        "aarch64" | "arm64" => {
            if os == "macos" {
                "arm64"
            } else {
                "aarch64"
            }
        }
        other => other,
    };
    format!("{os}-{arch}")
}

/// Load a manifest from a local file.
pub fn load_from_file(path: &Path) -> Result<ToolchainManifest, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read toolchain manifest {}: {e}", path.display()))?;
    parse_manifest(&text)
}

/// Parse and validate a manifest document.
pub fn parse_manifest(text: &str) -> Result<ToolchainManifest, String> {
    let manifest: ToolchainManifest =
        serde_json::from_str(text).map_err(|e| format!("Invalid toolchain manifest JSON: {e}"))?;
    if manifest.schema != 2 {
        return Err(format!(
            "Unsupported toolchain manifest schema `{}` (expected 2)",
            manifest.schema
        ));
    }
    if manifest.platforms.is_empty() {
        return Err("Toolchain manifest contains no platforms".to_string());
    }
    Ok(manifest)
}

/// Default manifest locations, in priority order. `None` when nothing exists
/// locally and the caller should fall back to [`DEFAULT_MANIFEST_URL`].
pub fn local_manifest_candidates(home: Option<&Path>) -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Some(url) = std::env::var_os("ADESH_TOOLCHAIN_MANIFEST") {
        candidates.push(std::path::PathBuf::from(url));
    }
    if let Some(home) = home {
        candidates.push(home.join("config").join("toolchain-manifest.json"));
    }
    // Development-tree convenience.
    candidates.push(
        std::path::PathBuf::from("installer")
            .join("manifests")
            .join("toolchain-manifest.json"),
    );
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
      "schema": 2,
      "adeshVersion": "0.3.0",
      "defaultComponents": ["llvm"],
      "components": {
        "llvm": { "version": "18.1.8", "required": true, "description": "AOT" },
        "mlir": { "version": "18.1.8-adesh1", "required": false, "description": "GPU" }
      },
      "platforms": {
        "windows-x86_64": {
          "llvm": { "url": "https://example.com/l.zip", "sha256": "aa", "size": 10, "format": "zip" }
        }
      }
    }"#;

    #[test]
    fn parses_and_validates_schema() {
        let m = parse_manifest(SAMPLE).expect("valid manifest");
        assert_eq!(m.schema, 2);
        assert_eq!(m.adesh_version, "0.3.0");
    }

    #[test]
    fn rejects_old_schema() {
        assert!(parse_manifest(r#"{"schema": 1}"#).is_err());
    }

    #[test]
    fn default_components_are_used_when_none_requested() {
        let m = parse_manifest(SAMPLE).unwrap();
        assert_eq!(m.resolve_components(&[]).unwrap(), vec!["llvm".to_string()]);
    }

    #[test]
    fn unknown_component_is_rejected() {
        let m = parse_manifest(SAMPLE).unwrap();
        assert!(m.resolve_components(&["nope".to_string()]).is_err());
    }

    #[test]
    fn platform_keys_match_dist_manifest() {
        assert_eq!(platform_key_for("windows", "x86_64"), "windows-x86_64");
        assert_eq!(platform_key_for("linux", "aarch64"), "linux-aarch64");
        assert_eq!(platform_key_for("macos", "aarch64"), "macos-arm64");
        assert_eq!(platform_key_for("macos", "x86_64"), "macos-x86_64");
    }

    #[test]
    fn embedded_manifest_is_valid() {
        let embedded = include_str!("../../installer/manifests/toolchain-manifest.json");
        let manifest = parse_manifest(embedded)
            .expect("embedded toolchain manifest must be valid JSON matching schema 2");
        assert_eq!(manifest.schema, 2);
        assert!(!manifest.platforms.is_empty());
        assert!(manifest.platforms.contains_key("linux-x86_64"));
        assert!(manifest.platforms.contains_key("windows-x86_64"));
        assert!(manifest.platforms.contains_key("macos-arm64"));
        assert!(manifest.platforms.contains_key("macos-x86_64"));
    }
}
