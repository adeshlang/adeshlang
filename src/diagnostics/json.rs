//! Machine-Readable Diagnostics JSON Output
//!
//! Provides stable JSON serialization of diagnostics for IDEs, CI, and tools.

use super::model::Diagnostic;
use serde::{Deserialize, Serialize};

pub const DIAGNOSTIC_SCHEMA_VERSION: &str = "1.0.0";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticJsonReport {
    pub schema_version: String,
    pub has_errors: bool,
    pub error_count: usize,
    pub warning_count: usize,
    pub diagnostics: Vec<Diagnostic>,
}

impl DiagnosticJsonReport {
    pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
        let error_count = diagnostics.iter().filter(|d| d.is_error()).count();
        let warning_count = diagnostics.iter().filter(|d| d.is_warning()).count();
        Self {
            schema_version: DIAGNOSTIC_SCHEMA_VERSION.to_string(),
            has_errors: error_count > 0,
            error_count,
            warning_count,
            diagnostics,
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}
