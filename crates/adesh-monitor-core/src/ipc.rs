//! IPC transport and client/server implementation for ARO
//!
//! Windows: Named Pipes (`\\.\pipe\adesh_observatory_ipc`)
//! Unix: Unix Domain Sockets (`/tmp/adesh_observatory.sock`)

use adesh_monitor_protocol::*;
use std::io::{BufRead, BufReader, Write};
#[cfg(not(windows))]
use std::time::Duration;

#[cfg(windows)]
pub const DEFAULT_IPC_ENDPOINT: &str = r"\\.\pipe\adesh_observatory_ipc";
#[cfg(not(windows))]
pub const DEFAULT_IPC_ENDPOINT: &str = "/tmp/adesh_observatory.sock";

/// The desktop app refreshes this file while it is open. Telemetry producers
/// consult it before allocating envelopes, opening IPC, or touching SQLite.
/// A short expiry means an uncleanly terminated desktop process cannot leave
/// background collection enabled indefinitely.
pub fn capture_lease_path() -> std::path::PathBuf {
    crate::storage::HistoryStore::default_db_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("observatory.capture")
}

pub fn capture_is_active() -> bool {
    // Allow opting out of telemetry via environment variable
    if std::env::var("ADESH_NO_OBSERVATORY")
        .map(|v| v == "1" || v == "true")
        .unwrap_or(false)
    {
        return false;
    }
    // Telemetry capture is active so that adesh build / run commands are reliably
    // logged to the durable SQLite history for ARO inspection.
    true
}

/// Non-blocking telemetry emitter used inside instrumented processes (`adesh`, `adeshlink`, runtimes).
/// If the collector is offline or not listening, it fails fast and silently (zero performance impact).
#[derive(Clone)]
pub struct TelemetryClient {
    session_id: String,
    pid: u32,
    start_instant: std::time::Instant,
}

impl TelemetryClient {
    pub fn new(session_id: String) -> Self {
        Self {
            session_id,
            pid: std::process::id(),
            start_instant: std::time::Instant::now(),
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn elapsed_micros(&self) -> u64 {
        self.start_instant.elapsed().as_micros() as u64
    }

    /// Try to send an event to the local collector endpoint. Non-blocking with timeout.
    pub fn emit(&self, payload: EventPayload) {
        // Observatory is opt-in at runtime: when the desktop app is closed no
        // telemetry allocations, IPC attempts, or SQLite writes occur.
        if !capture_is_active() {
            return;
        }
        let envelope = TelemetryEnvelope::new(
            self.session_id.clone(),
            self.pid,
            self.elapsed_micros(),
            payload,
        );

        let json_line = match envelope.to_json_line() {
            Ok(line) => line,
            Err(_) => return,
        };

        // If IPC send fails (no active collector daemon listening), fallback to direct SQLite WAL persistence
        if send_ipc_payload(&json_line).is_err() {
            let _ = crate::storage::HistoryStore::default_store()
                .map(|store| store.ingest_envelope(&envelope));
        }
    }
}

/// Send raw payload over platform IPC (Windows Named Pipe or Unix Socket) with timeout
fn send_ipc_payload(payload: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        let mut pipe = OpenOptions::new().write(true).open(DEFAULT_IPC_ENDPOINT)?;
        pipe.write_all(payload.as_bytes())?;
        pipe.flush()?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::net::UnixStream;
        let mut stream = UnixStream::connect(DEFAULT_IPC_ENDPOINT)?;
        stream.set_write_timeout(Some(Duration::from_millis(50)))?;
        stream.write_all(payload.as_bytes())?;
        stream.flush()?;
        Ok(())
    }
}

/// Send request and receive response (synchronous RPC for CLI/TUI/Desktop)
pub fn send_collector_rpc(req: &CollectorRequest) -> Result<CollectorResponse, String> {
    let req_json = serde_json::to_string(req).map_err(|e| e.to_string())? + "\n";

    #[cfg(windows)]
    {
        use std::fs::OpenOptions;
        let pipe = OpenOptions::new()
            .read(true)
            .write(true)
            .open(DEFAULT_IPC_ENDPOINT)
            .map_err(|e| {
                format!(
                    "Failed to connect to ARO Collector on {}: {}",
                    DEFAULT_IPC_ENDPOINT, e
                )
            })?;

        let mut writer = std::io::BufWriter::new(&pipe);
        writer
            .write_all(req_json.as_bytes())
            .map_err(|e| e.to_string())?;
        writer.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(&pipe);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        serde_json::from_str(&line).map_err(|e| format!("Invalid collector response: {}", e))
    }
    #[cfg(not(windows))]
    {
        use std::os::unix::net::UnixStream;
        let mut stream = UnixStream::connect(DEFAULT_IPC_ENDPOINT).map_err(|e| {
            format!(
                "Failed to connect to ARO Collector on {}: {}",
                DEFAULT_IPC_ENDPOINT, e
            )
        })?;
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .map_err(|e| e.to_string())?;

        stream
            .write_all(req_json.as_bytes())
            .map_err(|e| e.to_string())?;
        stream.flush().map_err(|e| e.to_string())?;

        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).map_err(|e| e.to_string())?;
        serde_json::from_str(&line).map_err(|e| format!("Invalid collector response: {}", e))
    }
}
