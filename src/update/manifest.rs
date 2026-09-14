//! Release manifest and installation metadata structures.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Update release channel
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UpdateChannel {
    Stable,
    Beta,
    Nightly,
}

impl Default for UpdateChannel {
    fn default() -> Self {
        Self::Stable
    }
}

impl UpdateChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Beta => "beta",
            Self::Nightly => "nightly",
        }
    }

    pub fn from_str_name(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "stable" => Some(Self::Stable),
            "beta" => Some(Self::Beta),
            "nightly" => Some(Self::Nightly),
            _ => None,
        }
    }
}

/// Known component identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentType {
    Compiler,
    Stdlib,
    Runtime,
    AiModel,
    Tools,
}

impl ComponentType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Compiler => "compiler",
            Self::Stdlib => "stdlib",
            Self::Runtime => "runtime",
            Self::AiModel => "ai_model",
            Self::Tools => "tools",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Compiler => "Adesh Compiler Binaries (adesh, adl, adesh-editor)",
            Self::Stdlib => "Standard Library Modules",
            Self::Runtime => "Static Runtime Libraries (adeshlang.lib)",
            Self::AiModel => "Embedded AI Model Weights",
            Self::Tools => "Developer Tools & Extensions",
        }
    }
}

/// Information about a single downloadable component in a release
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentInfo {
    pub version: String,
    pub size: u64,
    pub sha256: String,
    pub url: String,
    #[serde(default)]
    pub target_arch: Option<String>,
    #[serde(default)]
    pub target_os: Option<String>,
}

/// Remote release manifest format hosted on the release server
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseManifest {
    pub version: String,
    pub channel: UpdateChannel,
    pub released_at: String,
    #[serde(default)]
    pub min_updater_version: Option<String>,
    pub components: BTreeMap<String, ComponentInfo>,
}

impl ReleaseManifest {
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        serde_json::from_str(json_str).map_err(|e| format!("Invalid release manifest JSON: {e}"))
    }

    pub fn to_json_pretty(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| format!("Failed to serialize manifest: {e}"))
    }
}

/// Metadata describing currently installed components
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledComponent {
    pub version: String,
    pub sha256: String,
    pub updated_at: String,
}

/// The local `current.json` file stored in the Adesh installation root
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CurrentInstallation {
    pub version: String,
    pub channel: UpdateChannel,
    pub installed_at: String,
    pub last_checked_at: Option<String>,
    pub previous_version: Option<String>,
    pub components: BTreeMap<String, InstalledComponent>,
}

impl Default for CurrentInstallation {
    fn default() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            channel: UpdateChannel::Stable,
            installed_at: chrono_like_now(),
            last_checked_at: None,
            previous_version: None,
            components: BTreeMap::new(),
        }
    }
}

impl CurrentInstallation {
    /// Path to current.json inside the given installation home
    pub fn manifest_path(home: &Path) -> PathBuf {
        home.join("current.json")
    }

    /// Load current.json from installation directory, or generate a baseline default
    pub fn load_or_init(home: &Path) -> Self {
        let path = Self::manifest_path(home);
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(inst) = serde_json::from_str::<CurrentInstallation>(&content) {
                    return inst;
                }
            }
        }
        let mut baseline = Self::default();
        baseline.version = env!("CARGO_PKG_VERSION").to_string();
        baseline
    }

    /// Save current.json atomically
    pub fn save(&self, home: &Path) -> Result<(), String> {
        let path = Self::manifest_path(home);
        let temp_path = home.join(".current.json.tmp");
        let json_str = serde_json::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize current.json: {e}"))?;

        fs::write(&temp_path, json_str)
            .map_err(|e| format!("Failed to write temporary current.json: {e}"))?;

        if path.exists() {
            let _ = fs::remove_file(&path);
        }

        fs::rename(&temp_path, &path)
            .map_err(|e| format!("Failed to finalize current.json: {e}"))?;

        Ok(())
    }
}

/// Simple ISO-8601 UTC timestamp helper without heavy extra dependencies
pub fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{now}")
}
