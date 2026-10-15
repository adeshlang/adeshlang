//! `adesh-monitor-desktop`
//!
//! Native backend command handlers and IPC bridge for the Tauri 2 Desktop Observatory application.

use adesh_monitor_core::ipc::send_collector_rpc;
use adesh_monitor_core::storage::HistoryStore;
use adesh_monitor_protocol::*;

pub fn desktop_list_sessions(limit: usize) -> Result<Vec<SessionSummary>, String> {
    if let Ok(CollectorResponse::SessionsList { sessions }) =
        send_collector_rpc(&CollectorRequest::ListSessions { limit })
    {
        return Ok(sessions);
    }

    let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
    store.list_sessions(limit).map_err(|e| e.to_string())
}

pub fn desktop_get_session_detail(session_id: String) -> Result<SessionDetail, String> {
    if let Ok(CollectorResponse::SessionDetail { session }) =
        send_collector_rpc(&CollectorRequest::GetSessionDetail {
            session_id: session_id.clone(),
        })
    {
        return Ok(*session);
    }

    let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
    store
        .get_session_detail(&session_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("Session {} not found", session_id))
}

pub fn desktop_get_collector_stats() -> Result<CollectorStats, String> {
    if let Ok(CollectorResponse::Stats(stats)) =
        send_collector_rpc(&CollectorRequest::GetCollectorStats)
    {
        return Ok(stats);
    }

    let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
    store.get_stats().map_err(|e| e.to_string())
}

pub fn desktop_purge_history() -> Result<(), String> {
    let _ = send_collector_rpc(&CollectorRequest::PurgeHistory);
    let store = HistoryStore::default_store().map_err(|e| e.to_string())?;
    store.purge_history().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_desktop_list_and_stats() {
        let path = HistoryStore::default_db_path();
        println!("DB path: {:?}", path);
        let store = HistoryStore::default_store().unwrap();
        let stats = store.get_stats().unwrap();
        println!("Raw stats in DB: {:?}", stats);
        let sessions = desktop_list_sessions(10).unwrap();
        println!("Fetched sessions: {}", sessions.len());
    }
}
