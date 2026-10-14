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

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            get_sessions,
            get_session_detail,
            get_collector_stats,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Adesh Observatory application");
}
