//! Headway, one program with three ways to start it:
//!
//!   headway.exe              the app: a window around the UI in `ui/`
//!   headway.exe --mcp        the MCP server, talking on stdin/stdout (Claude starts it like this)
//!   headway.exe --uninstall  the window that removes Headway (Windows' "Installed apps" starts it like this)

// No console window behind the app on Windows release builds. Started with
// --mcp by another program, stdin and stdout are that program's pipes, which
// work the same without a console.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod uninstall;

use std::sync::Arc;

use headway_core::{Error, Service};
use serde_json::Value;
use tauri::{WebviewUrl, WebviewWindowBuilder};

/// The single door from the UI into the core: `invoke('api', { method, params })`.
/// The GitHub token never crosses it; it stays on the Rust side.
#[tauri::command]
async fn api(service: tauri::State<'_, Arc<Service>>, method: String, params: Option<Value>) -> Result<Value, Error> {
    service.call(&method, params.unwrap_or(Value::Null)).await
}

/// The UI files and settings compiled into the program (once, for every mode).
fn context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

fn app() {
    // Old copies of the program that an update had to leave behind while they ran.
    headway_install::sweep_beside_running_program();
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Arc::new(Service::new()))
        .invoke_handler(tauri::generate_handler![api])
        .setup(|app| {
            WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("Headway")
                .inner_size(1360.0, 880.0)
                .min_inner_size(980.0, 640.0)
                .center()
                .build()?;
            Ok(())
        })
        .run(context())
        .expect("Headway could not start");
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--mcp") => tauri::async_runtime::block_on(headway_mcp::serve_stdio()),
        Some("--uninstall") => uninstall::run(context()),
        _ => app(),
    }
}
