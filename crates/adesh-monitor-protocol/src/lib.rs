//! `adesh-monitor-protocol`
//!
//! Strongly-typed, versioned event schemas for Adesh Observatory (ARO).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Current protocol schema version
pub const PROTOCOL_VERSION: &str = "0.1.0";

/// Execution backend mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Interpreter,
    BytecodeVm,
    Jit,
    TieredJit,
    AdaptiveJit,
    AotNative,
    WebAssembly,
    Linker,
    Unknown,
}

/// Execution session state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Running,
    Completed,
    Failed,
    Terminated,
}

/// Process resource snapshot
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProcessMetrics {
    /// CPU usage percentage (can exceed 100% on multicore)
    pub cpu_percent: f32,
    /// Resident Set Size (RSS) / Working Set in bytes
    pub memory_rss_bytes: u64,
    /// Peak resident memory in bytes
    pub memory_peak_rss_bytes: u64,
    /// Virtual memory size in bytes
    pub memory_vms_bytes: u64,
    /// Active OS thread count
    pub thread_count: u32,
    /// Observed peak thread count
    pub peak_thread_count: u32,
    /// Total read bytes from disk
    pub disk_read_bytes: u64,
    /// Total written bytes to disk
    pub disk_write_bytes: u64,
    /// Accumulated process user CPU time in milliseconds
    pub cpu_user_ms: u64,
    /// Accumulated process system/kernel CPU time in milliseconds
    pub cpu_sys_ms: u64,
    /// Sample timestamp in UTC
    pub timestamp_utc: DateTime<Utc>,
    /// Monotonic timestamp in microseconds since session start
    pub elapsed_micros: u64,
}

/// Compilation or analysis stage timing
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CompilerStageEvent {
    pub stage_name: String,
    pub input_file: Option<String>,
    pub duration_micros: u64,
    pub success: bool,
    pub error_message: Option<String>,
    pub metadata: HashMap<String, String>,
}

/// Linker stage and artifact metrics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LinkerStageEvent {
    pub stage_name: String,
    pub target_triple: String,
    pub output_format: String,
    pub input_objects_count: usize,
    pub symbol_count: usize,
    pub section_count: usize,
    pub output_size_bytes: Option<u64>,
    pub duration_micros: u64,
    pub success: bool,
    pub error_message: Option<String>,
}

/// Runtime event (e.g. JIT compilation, allocation milestone, panic)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RuntimeEventPayload {
    pub event_name: String,
    pub component: String,
    pub duration_micros: Option<u64>,
    pub details: HashMap<String, String>,
}

/// Top-level observatory event variants
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", content = "payload", rename_all = "snake_case")]
pub enum EventPayload {
    SessionStart {
        tool_name: String,
        command_args: Vec<String>,
        working_dir: String,
        parent_pid: Option<u32>,
        execution_mode: ExecutionMode,
        target_file: Option<String>,
    },
    SessionEnd {
        exit_code: i32,
        duration_micros: u64,
        status: SessionStatus,
        error_message: Option<String>,
    },
    MetricsSample(ProcessMetrics),
    CompilerStage(CompilerStageEvent),
    LinkerStage(LinkerStageEvent),
    RuntimeEvent(RuntimeEventPayload),
    Heartbeat,
}

/// The versioned envelope wrapping every telemetry event sent over IPC
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TelemetryEnvelope {
    pub schema_version: String,
    pub event_id: String,
    pub session_id: String,
    pub pid: u32,
    pub timestamp_utc: DateTime<Utc>,
    pub monotonic_micros: u64,
    pub payload: EventPayload,
}

impl TelemetryEnvelope {
    pub fn new(session_id: String, pid: u32, monotonic_micros: u64, payload: EventPayload) -> Self {
        Self {
            schema_version: PROTOCOL_VERSION.to_string(),
            event_id: uuid::Uuid::new_v4().to_string(),
            session_id,
            pid,
            timestamp_utc: Utc::now(),
            monotonic_micros,
            payload,
        }
    }

    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let mut s = serde_json::to_string(self)?;
        s.push('\n');
        Ok(s)
    }

    pub fn from_json_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// IPC commands sent from CLI/TUI/Desktop clients to the Observatory Collector
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum CollectorRequest {
    Ping,
    ListSessions { limit: usize },
    GetSessionDetail { session_id: String },
    GetSessionMetrics { session_id: String, limit: usize },
    GetSessionStages { session_id: String },
    GetCollectorStats,
    PurgeHistory,
}

/// Summarized execution session representation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionSummary {
    pub session_id: String,
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub tool_name: String,
    pub execution_mode: ExecutionMode,
    pub command_line: String,
    pub status: SessionStatus,
    pub exit_code: Option<i32>,
    pub start_time: DateTime<Utc>,
    pub end_time: Option<DateTime<Utc>>,
    pub duration_ms: Option<u64>,
    pub peak_rss_bytes: u64,
    pub cpu_peak_percent: f32,
    /// Number of secrets masked in `command_line` before it was persisted
    #[serde(default)]
    pub redaction_count: u32,
}

/// Metadata describing a built-in redaction rule (exposed to UI clients)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RedactionRuleInfo {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Human-readable example of what the rule masks, e.g. `--password=***`
    pub example: String,
    /// One of `critical`, `high`, `medium`
    pub severity: String,
}

/// A single secret masked by the redaction engine
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RedactionFinding {
    pub rule_id: String,
    pub rule_label: String,
    /// The redacted fragment as stored, e.g. `--password=***`
    pub masked_preview: String,
}

/// One row in the redaction audit trail
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuditEntry {
    pub id: i64,
    pub session_id: String,
    pub tool_name: String,
    /// Persisted (already masked) command line for the session
    pub command_line: String,
    pub rule_id: String,
    pub rule_label: String,
    pub masked_preview: String,
    /// RFC 3339 timestamp of when the secret was masked
    pub redacted_at: String,
}

/// Aggregated security posture for the Security & Redaction tab
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SecurityReport {
    /// All built-in redaction rules with their current match totals
    pub rules: Vec<RedactionRuleInfo>,
    /// rule_id -> number of times this rule masked a secret
    pub findings_by_rule: HashMap<String, u64>,
    /// session_id -> number of secrets masked in that session
    pub redaction_counts: HashMap<String, u64>,
    pub total_sessions_scanned: usize,
    pub sessions_with_redactions: usize,
    pub total_redactions: u64,
    /// Most recent redaction events, newest first
    pub audit_entries: Vec<AuditEntry>,
    /// RFC 3339 timestamp of the last manual history rescan
    pub last_rescan: Option<String>,
    pub rescan_count: u64,
    /// Absolute path of the SQLite audit database at rest
    pub db_path: String,
    /// True when secrets are masked automatically at ingest time
    pub ingest_redaction_enabled: bool,
}

/// Detailed session report
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionDetail {
    pub summary: SessionSummary,
    pub working_dir: String,
    pub target_file: Option<String>,
    pub compiler_stages: Vec<CompilerStageEvent>,
    pub linker_stages: Vec<LinkerStageEvent>,
    pub runtime_events: Vec<RuntimeEventPayload>,
    pub metrics_count: usize,
    pub latest_metrics: Option<ProcessMetrics>,
}

/// Collector statistics
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CollectorStats {
    pub uptime_secs: u64,
    pub total_events_ingested: u64,
    pub total_events_dropped: u64,
    pub active_sessions_count: usize,
    pub stored_sessions_count: usize,
    pub database_size_bytes: u64,
}

/// Responses sent by the Observatory Collector back to clients
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CollectorResponse {
    Ok,
    Pong {
        version: String,
        uptime_secs: u64,
    },
    SessionsList {
        sessions: Vec<SessionSummary>,
    },
    SessionDetail {
        session: Box<SessionDetail>,
    },
    SessionMetrics {
        session_id: String,
        metrics: Vec<ProcessMetrics>,
    },
    SessionStages {
        session_id: String,
        compiler_stages: Vec<CompilerStageEvent>,
        linker_stages: Vec<LinkerStageEvent>,
    },
    Stats(CollectorStats),
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_serialization_roundtrip() {
        let envelope = TelemetryEnvelope::new(
            "sess-123".to_string(),
            4567,
            1000,
            EventPayload::SessionStart {
                tool_name: "adesh".to_string(),
                command_args: vec!["run".to_string(), "main.adesh".to_string()],
                working_dir: "D:\\test".to_string(),
                parent_pid: Some(100),
                execution_mode: ExecutionMode::Jit,
                target_file: Some("main.adesh".to_string()),
            },
        );

        let json_line = envelope.to_json_line().expect("must serialize");
        assert!(json_line.ends_with('\n'));

        let decoded = TelemetryEnvelope::from_json_str(&json_line).expect("must deserialize");
        assert_eq!(decoded.session_id, "sess-123");
        assert_eq!(decoded.pid, 4567);
        if let EventPayload::SessionStart { tool_name, .. } = decoded.payload {
            assert_eq!(tool_name, "adesh");
        } else {
            panic!("Unexpected payload kind");
        }
    }

    #[test]
    fn test_request_response_roundtrip() {
        let req = CollectorRequest::GetSessionDetail {
            session_id: "sess-999".to_string(),
        };
        let s = serde_json::to_string(&req).expect("req serialize");
        let decoded_req: CollectorRequest = serde_json::from_str(&s).expect("req deserialize");
        assert_eq!(req, decoded_req);

        let resp = CollectorResponse::Pong {
            version: PROTOCOL_VERSION.to_string(),
            uptime_secs: 42,
        };
        let s2 = serde_json::to_string(&resp).expect("resp serialize");
        let decoded_resp: CollectorResponse = serde_json::from_str(&s2).expect("resp deserialize");
        assert_eq!(resp, decoded_resp);
    }
}
