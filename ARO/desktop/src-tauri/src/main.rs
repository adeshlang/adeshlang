// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use adesh_monitor_desktop::*;
use adesh_monitor_protocol::*;

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
fn get_collector_stats() -> Result<CollectorStats, String> {
    desktop_get_collector_stats()
}

#[tauri::command]
fn purge_history() -> Result<(), String> {
    desktop_purge_history()
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
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_session_detail,
            get_collector_stats,
            purge_history,
            get_hardware_info,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Adesh Observatory application");
}
