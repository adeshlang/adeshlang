// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use adesh_monitor_desktop::*;
use adesh_monitor_protocol::*;

#[tauri::command]
fn get_sessions(limit: Option<usize>) -> Result<Vec<SessionSummary>, String> {
    desktop_list_sessions(limit.unwrap_or(50))
}

#[tauri::command]
fn get_session_detail(session_id: String) -> Result<SessionDetail, String> {
    desktop_get_session_detail(session_id)
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
fn get_hardware_info() -> Result<serde_json::Value, String> {
    let provider = adesh_monitor_platform::create_platform_provider();
    let caps = provider.query_capabilities();
    Ok(serde_json::json!({
        "os_name": std::env::consts::OS,
        "arch": std::env::consts::ARCH,
        "logical_cores": std::thread::available_parallelism().map(|p| p.get()).unwrap_or(1),
        "supports_process_cpu": caps.supports_process_cpu,
        "supports_process_memory": caps.supports_process_memory,
        "supports_disk_io": caps.supports_disk_io,
        "supports_accelerators": caps.supports_accelerators,
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
