//! `headway.exe --uninstall`: the small window that removes Headway. Windows'
//! "Installed apps" starts the program this way.
//!
//! A program cannot delete its own file while it runs. So the window removes
//! everything else at once (shortcuts, the Installed apps entry, optionally
//! the settings), moves the program file out of its folder, and when the
//! window closes hands the file itself to Windows to delete a moment later.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use headway_install as install;
use serde::Serialize;
use tauri::{Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// What is left to delete once this program has ended.
#[derive(Default)]
struct Later {
    files: Vec<PathBuf>,
    trees: Vec<PathBuf>,
    empty_dirs: Vec<PathBuf>,
}

struct State {
    later: Mutex<Later>,
    /// Where this window's browser engine keeps its files: a temporary folder,
    /// so the app's own one is free to be deleted.
    web_data: PathBuf,
    /// The removal is under way: the window stays open until it is done.
    busy: AtomicBool,
}

/// Marks the state busy for as long as it lives.
struct Busy<'a>(&'a AtomicBool);

impl<'a> Busy<'a> {
    fn start(flag: &'a AtomicBool) -> Self {
        flag.store(true, Ordering::SeqCst);
        Busy(flag)
    }
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Info {
    version: &'static str,
    /// What Windows has on record; `None` when Headway is not installed.
    installed: Option<install::Installed>,
    /// The language the app is set to, if one was chosen.
    language: Option<String>,
}

/// An error the window can show: `{ code, message }`, like the core's.
#[derive(Serialize)]
struct Failure {
    code: &'static str,
    message: String,
}

impl From<install::Error> for Failure {
    fn from(e: install::Error) -> Self {
        Failure { code: e.code(), message: e.to_string() }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    /// Things that could not be removed (the uninstall itself still happened).
    problems: Vec<String>,
}

#[tauri::command]
fn uninstall_info() -> Info {
    let app_id = install::Layout::standard().map(|l| l.app_id).unwrap_or_default();
    Info {
        version: env!("CARGO_PKG_VERSION"),
        installed: install::installed(&app_id),
        language: install::config_file().and_then(|f| install::read_language(&f)),
    }
}

#[tauri::command]
async fn uninstall_run(app: tauri::AppHandle, state: tauri::State<'_, State>, remove_data: bool) -> Result<Done, Failure> {
    let _busy = Busy::start(&state.busy);
    // The app's own browser-engine files (a cache, no settings in it).
    let web_cache = if install::sandbox().is_some() { None } else { app.path().app_local_data_dir().ok() };

    let work = move || -> Result<(install::Removal, Vec<PathBuf>, Vec<String>), install::Error> {
        let layout = install::Layout::of_existing()?;
        let left = install::uninstall(&layout)?;
        let mut trees = Vec::new();
        if let Some(dir) = web_cache {
            if dir.exists() && std::fs::remove_dir_all(&dir).is_err() {
                trees.push(dir);
            }
        }
        let mut problems = Vec::new();
        if remove_data {
            // A trial run in a sandbox must never reach the real settings.
            if install::sandbox().is_some() && std::env::var_os("HEADWAY_CONFIG_DIR").is_none() {
                problems.push("Sandbox: the settings were left alone (HEADWAY_CONFIG_DIR is not set).".to_string());
            } else {
                problems = headway_core::config::erase_local_data();
            }
        }
        Ok((left, trees, problems))
    };
    let (left, trees, problems) = tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| Failure { code: "failed", message: e.to_string() })??;

    let mut later = state.later.lock().unwrap_or_else(|e| e.into_inner());
    later.files.extend(left.files);
    later.empty_dirs.extend(left.empty_dirs);
    later.trees.extend(trees);
    Ok(Done { problems })
}

pub fn run(context: tauri::Context<tauri::Wry>) {
    let web_data = std::env::temp_dir().join(format!("Headway-uninstall-{}", std::process::id()));
    let for_window = web_data.clone();
    let app = tauri::Builder::default()
        .manage(State { later: Mutex::new(Later::default()), web_data, busy: AtomicBool::new(false) })
        .invoke_handler(tauri::generate_handler![uninstall_info, uninstall_run])
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "uninstall", WebviewUrl::App("uninstall.html".into()))
                .title("Headway")
                .inner_size(560.0, 430.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .shadow(true)
                .center()
                .data_directory(for_window)
                .build()?;
            Ok(())
        })
        .build(context)
        .expect("Headway could not start");

    app.run(|handle, event| match event {
        // Closing the window in the middle of the removal would leave it half done.
        RunEvent::WindowEvent { event: tauri::WindowEvent::CloseRequested { api, .. }, .. } => {
            if handle.state::<State>().busy.load(Ordering::SeqCst) {
                api.prevent_close();
            }
        }
        RunEvent::Exit => {
            let state = handle.state::<State>();
            let later = state.later.lock().unwrap_or_else(|e| e.into_inner());
            let mut trees = later.trees.clone();
            trees.push(state.web_data.clone());
            let _ = install::delete_after_exit(&later.files, &trees, &later.empty_dirs);
        }
        _ => {}
    });
}
