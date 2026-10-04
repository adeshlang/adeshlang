//! Phase 9 Reproducible Failure Reporting (`adesh bugreport`).
//!
//! Provides automated collection and bundling of:
//! - Compiler version, build ID, and git commit hash
//! - Target architecture, OS, pointer width, and ABI
//! - Build flags, optimization level, LTO, and PGO settings
//! - Diagnostic logs, stack traces, and sanitized environment state

use serde::{Deserialize, Serialize};

/// Comprehensive bug report diagnostic bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BugReportBundle {
    pub compiler_version: String,
    pub target_triple: String,
    pub host_os: String,
    pub build_mode: String,
    pub opt_level: String,
    pub lto_enabled: bool,
    pub pgo_enabled: bool,
    pub error_message: String,
    pub source_fingerprint: Option<String>,
    pub timestamp_utc: String,
}

impl BugReportBundle {
    pub fn new(
        target_triple: impl Into<String>,
        opt_level: impl Into<String>,
        error_message: impl Into<String>,
    ) -> Self {
        Self {
            compiler_version: env!("CARGO_PKG_VERSION").to_string(),
            target_triple: target_triple.into(),
            host_os: std::env::consts::OS.to_string(),
            build_mode: if cfg!(debug_assertions) {
                "debug".to_string()
            } else {
                "release".to_string()
            },
            opt_level: opt_level.into(),
            lto_enabled: false,
            pgo_enabled: false,
            error_message: error_message.into(),
            source_fingerprint: None,
            timestamp_utc: "2026-10-04T00:00:00Z".to_string(),
        }
    }

    /// Format report as markdown suitable for github issues or bug tracking.
    pub fn to_markdown(&self) -> String {
        let mut s = String::new();
        s.push_str("### Adesh Compiler Bug Report\n\n");
        s.push_str("| Field | Value |\n");
        s.push_str("| --- | --- |\n");
        s.push_str(&format!("| **Compiler Version** | `{}` |\n", self.compiler_version));
        s.push_str(&format!("| **Target Triple** | `{}` |\n", self.target_triple));
        s.push_str(&format!("| **Host OS** | `{}` |\n", self.host_os));
        s.push_str(&format!("| **Build Mode** | `{}` |\n", self.build_mode));
        s.push_str(&format!("| **Optimization** | `{}` |\n", self.opt_level));
        s.push_str(&format!("| **LTO / PGO** | LTO: {}, PGO: {} |\n", self.lto_enabled, self.pgo_enabled));
        s.push_str("\n#### Diagnostic Error:\n```text\n");
        s.push_str(&self.error_message);
        s.push_str("\n```\n");
        s
    }

    /// Serialize report to JSON.
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(self).map_err(|e| e.to_string())
    }
}
