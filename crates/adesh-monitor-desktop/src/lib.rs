//! `adesh-monitor-desktop`
//!
//! Native backend command handlers and IPC bridge for the Tauri 2 Desktop Observatory application.

use adesh_monitor_core::ipc::send_collector_rpc;
use adesh_monitor_protocol::*;

pub fn desktop_list_sessions(limit: usize) -> Result<Vec<SessionSummary>, String> {
    match send_collector_rpc(&CollectorRequest::ListSessions { limit })? {
        CollectorResponse::SessionsList { sessions } => Ok(sessions),
        CollectorResponse::Error { message } => Err(message),
        _ => Err("Unexpected response".to_string()),
    }
}

pub fn desktop_get_session_detail(session_id: String) -> Result<SessionDetail, String> {
    match send_collector_rpc(&CollectorRequest::GetSessionDetail { session_id })? {
        CollectorResponse::SessionDetail { session } => Ok(*session),
        CollectorResponse::Error { message } => Err(message),
        _ => Err("Unexpected response".to_string()),
    }
}

pub fn desktop_get_collector_stats() -> Result<CollectorStats, String> {
    match send_collector_rpc(&CollectorRequest::GetCollectorStats)? {
        CollectorResponse::Stats(stats) => Ok(stats),
        CollectorResponse::Error { message } => Err(message),
        _ => Err("Unexpected response".to_string()),
    }
}
