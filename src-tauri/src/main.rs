//! The Headway desktop app: a window around the UI in `ui/`, with one command
//! that forwards every request to the shared core.

// No console window behind the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use headway_core::{Error, Service};
use serde_json::Value;

/// The single door from the UI into the core: `invoke('api', { method, params })`.
/// The GitHub token never crosses it; it stays on the Rust side.
#[tauri::command]
async fn api(service: tauri::State<'_, Arc<Service>>, method: String, params: Option<Value>) -> Result<Value, Error> {
    service.call(&method, params.unwrap_or(Value::Null)).await
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Service::new()))
        .invoke_handler(tauri::generate_handler![api])
        .run(tauri::generate_context!())
        .expect("Headway could not start");
}
