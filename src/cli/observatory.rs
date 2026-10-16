//! `adesh monitor` CLI integration module
//!
//! Provides the entry point for running the interactive TUI or scriptable CLI
//! through the main `adesh` binary, plus automatic session lifecycle hooks.

use adesh_monitor_core::ipc::TelemetryClient;
use adesh_monitor_protocol::*;
use std::sync::Mutex;
use std::time::Instant;

static ACTIVE_SESSION: Mutex<Option<(TelemetryClient, Instant)>> = Mutex::new(None);

pub struct SessionGuard {
    pub client: TelemetryClient,
    pub start_instant: Instant,
}

impl Drop for SessionGuard {
    fn drop(&mut self) {
        finish_observatory_session(if std::thread::panicking() { 101 } else { 0 }, None);
    }
}

pub fn finish_observatory_session(exit_code: i32, error_message: Option<&str>) {
    if let Ok(mut lock) = ACTIVE_SESSION.lock() {
        if let Some((client, start_instant)) = lock.take() {
            let dur = start_instant.elapsed().as_micros() as u64;
            // Short compiler invocations often finish before the collector's
            // periodic sampler fires. Capture one final process snapshot so
            // CPU/RSS diagnostics are never silently reported as zero.
            if let Some(metrics) = adesh_monitor_platform::create_platform_provider()
                .sample_process(std::process::id(), dur)
            {
                client.emit(EventPayload::MetricsSample(metrics));
            }
            client.emit(EventPayload::SessionEnd {
                exit_code,
                duration_micros: dur,
                status: if exit_code == 0 {
                    SessionStatus::Completed
                } else {
                    SessionStatus::Failed
                },
                error_message: error_message.map(|s| s.to_string()),
            });
        }
    }
}

pub fn init_observatory_session(
    command: &str,
    input_file: Option<&str>,
    args: &[String],
    execution_mode: ExecutionMode,
) -> Option<SessionGuard> {
    if matches!(
        command,
        "monitor"
            | "aro"
            | "observatory"
            | "doctor"
            | "help"
            | "--help"
            | "-h"
            | "version"
            | "--version"
            | "-v"
    ) {
        return None;
    }

    let session_id = std::env::var("ADESH_OBSERVATORY_SESSION")
        .unwrap_or_else(|_| format!("sess-{}", uuid::Uuid::new_v4()));

    unsafe {
        std::env::set_var("ADESH_OBSERVATORY_SESSION", &session_id);
    }

    let client = TelemetryClient::new(session_id);
    let working_dir = std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| ".".to_string());

    client.emit(EventPayload::SessionStart {
        tool_name: "adesh".to_string(),
        command_args: args.to_vec(),
        working_dir,
        parent_pid: None,
        execution_mode,
        target_file: input_file.map(|s| s.to_string()),
    });

    let now = Instant::now();
    if let Ok(mut lock) = ACTIVE_SESSION.lock() {
        *lock = Some((client.clone(), now));
    }

    Some(SessionGuard {
        client,
        start_instant: now,
    })
}

pub fn execute_monitor_command(args: &[String]) {
    if args.is_empty() {
        // Default to launching interactive TUI if no subcommands provided
        if let Err(e) = adesh_monitor_tui::run_tui() {
            eprintln!("Failed to launch Observatory TUI: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if args[0] == "tui" || args[0] == "interactive" {
        if let Err(e) = adesh_monitor_tui::run_tui() {
            eprintln!("Failed to launch Observatory TUI: {}", e);
            std::process::exit(1);
        }
        return;
    }

    if let Err(e) = adesh_monitor_cli::run_cli(args) {
        eprintln!("Observatory Error: {}", e);
        std::process::exit(1);
    }
}
