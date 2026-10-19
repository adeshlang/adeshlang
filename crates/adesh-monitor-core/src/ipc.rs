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

/// Default lease expiration timeout in seconds.
/// The desktop or collector process refreshes its lease every 5s; 15s allows for transient lag.
pub const LEASE_EXPIRY_SECS: u64 = 15;

/// State of an observatory capture lease on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaseState {
    /// Active lease held by a currently running desktop or collector process.
    Active { pid: u32 },
    /// No lease file exists.
    Missing,
    /// Lease file exists but its timestamp / modification time has expired.
    Expired,
    /// Lease file content is empty or cannot be parsed.
    Malformed,
    /// Lease file points to a PID that is no longer running (abnormal process termination).
    StaleDeadPid { pid: u32 },
}

/// The desktop app or CLI collector refreshes this file while active. Telemetry producers
/// consult it before allocating envelopes, opening IPC, or touching SQLite.
/// A short expiry means an uncleanly terminated process cannot leave background collection enabled indefinitely.
pub fn capture_lease_path() -> std::path::PathBuf {
    crate::storage::HistoryStore::default_db_path()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("observatory.capture")
}

/// Inspect the liveness of a process by PID across platforms without external tools.
#[cfg(windows)]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
    };
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle == 0 {
            return false;
        }
        let mut exit_code: u32 = 0;
        let ok = GetExitCodeProcess(handle, &mut exit_code);
        CloseHandle(handle);
        ok != 0 && exit_code == 259 // STILL_ACTIVE (0x103)
    }
}

#[cfg(not(windows))]
pub fn is_pid_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        let ret = libc::kill(pid as libc::pid_t, 0);
        if ret == 0 {
            true
        } else {
            let err = std::io::Error::last_os_error();
            err.raw_os_error() == Some(libc::EPERM)
        }
    }
}

/// Inspect and validate a capture lease file on disk.
/// Automatically cleans up stale or malformed lease files when safe to do so.
pub fn inspect_lease_file(path: &std::path::Path) -> LeaseState {
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return LeaseState::Missing,
    };

    // Check modification time expiration
    if let Ok(mod_time) = meta.modified()
        && let Ok(elapsed) = std::time::SystemTime::now().duration_since(mod_time)
        && elapsed.as_secs() > LEASE_EXPIRY_SECS
    {
        let _ = std::fs::remove_file(path);
        return LeaseState::Expired;
    }

    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return LeaseState::Malformed,
    };

    let trimmed = content.trim();
    if trimmed.is_empty() {
        let _ = std::fs::remove_file(path);
        return LeaseState::Malformed;
    }

    let mut parsed_pid = None;
    for line in trimmed.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("desktop_pid=") {
            if let Ok(p) = rest.trim().parse::<u32>() {
                parsed_pid = Some(p);
                break;
            }
        } else if let Some(rest) = line.strip_prefix("pid=") {
            if let Ok(p) = rest.trim().parse::<u32>() {
                parsed_pid = Some(p);
                break;
            }
        } else if let Ok(p) = line.parse::<u32>() {
            parsed_pid = Some(p);
            break;
        }
    }

    let owner_pid = match parsed_pid {
        Some(p) => p,
        None => {
            let _ = std::fs::remove_file(path);
            return LeaseState::Malformed;
        }
    };

    if !is_pid_alive(owner_pid) {
        let _ = std::fs::remove_file(path);
        return LeaseState::StaleDeadPid { pid: owner_pid };
    }

    LeaseState::Active { pid: owner_pid }
}

/// Create or atomically refresh a capture lease for the current process or designated PID.
pub fn create_capture_lease(pid: u32) -> std::io::Result<()> {
    let path = capture_lease_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    static LEASE_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let counter_val = LEASE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp_path = path.with_extension(format!(
        "tmp.{}.{}.{}",
        pid,
        std::process::id(),
        counter_val
    ));
    let content = format!("desktop_pid={}\nupdated_at={}\n", pid, epoch);
    std::fs::write(&tmp_path, content.as_bytes())?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Renew an existing capture lease, updating its heartbeat on disk.
pub fn renew_capture_lease(pid: u32) -> std::io::Result<()> {
    create_capture_lease(pid)
}

/// Explicitly release and remove the capture lease file.
pub fn release_capture_lease() -> std::io::Result<()> {
    let path = capture_lease_path();
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// Determine whether observatory telemetry capture is active.
///
/// Priority order:
/// 1. `ADESH_NO_OBSERVATORY` environment variable disables telemetry unconditionally.
/// 2. `ADESH_OBSERVATORY_CAPTURE` or `ADESH_CAPTURE_ALL` forces telemetry active (e.g. for CI/headless tests).
/// 3. Validated on-disk capture lease (checked for expiry and process liveness).
pub fn capture_is_active_with_env<F>(mut get_env: F) -> bool
where
    F: FnMut(&str) -> Option<String>,
{
    // 1. Opt-out takes highest precedence
    if get_env("ADESH_NO_OBSERVATORY")
        .map(|v| v == "1" || v == "true" || v == "yes")
        .unwrap_or(false)
    {
        return false;
    }

    // 2. Explicit opt-in / test overrides
    if get_env("ADESH_OBSERVATORY_CAPTURE")
        .or_else(|| get_env("ADESH_CAPTURE_ALL"))
        .map(|v| v == "1" || v == "true" || v == "always")
        .unwrap_or(false)
    {
        return true;
    }

    // 3. Genuine lease validation
    matches!(
        inspect_lease_file(&capture_lease_path()),
        LeaseState::Active { .. }
    )
}

pub fn capture_is_active() -> bool {
    capture_is_active_with_env(|k| std::env::var(k).ok())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_pid_alive_current_process() {
        assert!(is_pid_alive(std::process::id()));
        assert!(!is_pid_alive(0));
        assert!(!is_pid_alive(999_999_999));
    }

    #[test]
    fn test_inspect_lease_missing() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("missing.capture");
        assert_eq!(inspect_lease_file(&path), LeaseState::Missing);
    }

    #[test]
    fn test_inspect_lease_active() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("active.capture");
        let my_pid = std::process::id();
        std::fs::write(&path, format!("desktop_pid={}\n", my_pid)).unwrap();
        assert_eq!(
            inspect_lease_file(&path),
            LeaseState::Active { pid: my_pid }
        );
    }

    #[test]
    fn test_inspect_lease_malformed_and_cleanup() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("malformed.capture");
        std::fs::write(&path, "not_a_valid_pid_content").unwrap();
        assert_eq!(inspect_lease_file(&path), LeaseState::Malformed);
        assert!(!path.exists(), "malformed lease file should be cleaned up");
    }

    #[test]
    fn test_inspect_lease_dead_pid_stale_recovery() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("dead.capture");
        let dead_pid = 999_999_999;
        std::fs::write(&path, format!("desktop_pid={}\n", dead_pid)).unwrap();
        assert_eq!(
            inspect_lease_file(&path),
            LeaseState::StaleDeadPid { pid: dead_pid }
        );
        assert!(
            !path.exists(),
            "stale lease file with dead PID should be cleaned up"
        );
    }

    #[test]
    fn test_capture_is_active_opt_out_precedence() {
        let active = capture_is_active_with_env(|k| match k {
            "ADESH_NO_OBSERVATORY" => Some("1".to_string()),
            "ADESH_OBSERVATORY_CAPTURE" => Some("1".to_string()),
            _ => None,
        });
        assert!(!active);
    }

    #[test]
    fn test_capture_is_active_explicit_opt_in() {
        let active = capture_is_active_with_env(|k| match k {
            "ADESH_NO_OBSERVATORY" => None,
            "ADESH_OBSERVATORY_CAPTURE" => Some("1".to_string()),
            _ => None,
        });
        assert!(active);
    }

    #[test]
    fn test_atomic_lease_creation_and_renewal() {
        let my_pid = std::process::id();
        let res = create_capture_lease(my_pid);
        assert!(res.is_ok());
        let path = capture_lease_path();
        assert!(path.exists());
        let res2 = renew_capture_lease(my_pid);
        assert!(res2.is_ok());
        let state = inspect_lease_file(&path);
        assert_eq!(state, LeaseState::Active { pid: my_pid });
        let _ = release_capture_lease();
    }

    #[test]
    fn test_inspect_lease_expired() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("expired.capture");
        let my_pid = std::process::id();
        std::fs::write(&path, format!("desktop_pid={}\n", my_pid)).unwrap();
        // Set modification time to 30 seconds ago
        let old_time = std::time::SystemTime::now() - std::time::Duration::from_secs(30);
        let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
        file.set_modified(old_time).unwrap();
        drop(file);
        assert_eq!(inspect_lease_file(&path), LeaseState::Expired);
        assert!(!path.exists(), "expired lease file should be cleaned up");
    }

    #[test]
    fn test_concurrent_lease_renewals() {
        let my_pid = std::process::id();
        let mut handles = Vec::new();
        for _ in 0..10 {
            handles.push(std::thread::spawn(move || {
                let _ = renew_capture_lease(my_pid);
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        let path = capture_lease_path();
        assert_eq!(
            inspect_lease_file(&path),
            LeaseState::Active { pid: my_pid }
        );
        let _ = release_capture_lease();
    }
}
