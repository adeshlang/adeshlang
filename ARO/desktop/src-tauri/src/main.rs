// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use adesh_monitor_desktop::*;
use adesh_monitor_protocol::*;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc};
use std::time::Duration;

#[tauri::command]
fn get_sessions(limit: Option<usize>) -> Result<Vec<SessionSummary>, String> {
    let lim = limit.unwrap_or(50);
    let sessions = desktop_list_sessions(lim)?;
    Ok(sessions)
}

#[tauri::command]
#[allow(non_snake_case)]
fn get_session_detail(
    session_id: Option<String>,
    sessionId: Option<String>,
) -> Result<SessionDetail, String> {
    let id = session_id
        .or(sessionId)
        .ok_or_else(|| "Session ID parameter missing".to_string())?;
    desktop_get_session_detail(id)
}

#[tauri::command]
fn get_session_metrics(session_id: String, limit: Option<usize>) -> Result<Vec<ProcessMetrics>, String> {
    let limit = limit.unwrap_or(500);
    if let Ok(CollectorResponse::SessionMetrics { metrics, .. }) =
        adesh_monitor_core::ipc::send_collector_rpc(&CollectorRequest::GetSessionMetrics {
            session_id: session_id.clone(),
            limit,
        })
    {
        return Ok(metrics);
    }
    let store = adesh_monitor_core::storage::HistoryStore::default_store().map_err(|e| e.to_string())?;
    store.get_metrics(&session_id, limit).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_collector_stats() -> Result<CollectorStats, String> {
    desktop_get_collector_stats()
}

#[tauri::command]
fn purge_history() -> Result<(), String> {
    desktop_purge_history()
}

#[tauri::command]
fn get_security_report() -> Result<SecurityReport, String> {
    desktop_get_security_report()
}

#[tauri::command]
fn rescan_history() -> Result<SecurityReport, String> {
    desktop_rescan_history()
}

#[tauri::command]
async fn get_hardware_info() -> Result<serde_json::Value, String> {
    let caps = adesh_monitor_platform::create_platform_provider().query_capabilities();
    let report = adesh_monitor_platform::hardware::detect_hardware();
    Ok(serde_json::json!({
        "os_name": report.os_name,
        "arch": report.arch,
        "cpu_name": report.cpu.name,
        "logical_cores": report.cpu.logical_cores,
        "physical_cores": report.cpu.physical_cores,
        "cpu_max_clock_mhz": report.cpu.max_clock_mhz,
        "total_memory_bytes": report.total_memory_bytes,
        "available_memory_bytes": report.available_memory_bytes,
        "gpus": report.gpus,
        "gpu_detection_method": report.gpu_detection_method,
        "supports_process_cpu": caps.supports_process_cpu,
        "supports_process_memory": caps.supports_process_memory,
        "supports_disk_io": caps.supports_disk_io,
        "supports_accelerators": !report.gpus.is_empty(),
    }))
}

fn main() {
    let capture_lease = adesh_monitor_core::ipc::capture_lease_path();
    let _ = std::fs::create_dir_all(capture_lease.parent().unwrap_or_else(|| std::path::Path::new(".")));
    let _ = std::fs::write(&capture_lease, format!("desktop_pid={}", std::process::id()));
    let lease_running = Arc::new(AtomicBool::new(true));
    let lease_heartbeat = lease_running.clone();
    let lease_path = capture_lease.clone();
    std::thread::spawn(move || {
        while lease_heartbeat.load(Ordering::Relaxed) {
            let _ = std::fs::write(&lease_path, format!("desktop_pid={}", std::process::id()));
            std::thread::sleep(Duration::from_secs(5));
        }
    });

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_session_detail,
            get_session_metrics,
            get_collector_stats,
            purge_history,
            get_security_report,
            rescan_history,
            get_hardware_info,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Adesh Observatory application")
        .run(move |_app_handle, event| {
            if matches!(event, tauri::RunEvent::Exit | tauri::RunEvent::ExitRequested { .. }) {
                lease_running.store(false, Ordering::Relaxed);
                let _ = std::fs::remove_file(&capture_lease);
            }
        });
}
