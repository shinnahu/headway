//! Headway Setup: one window that installs Headway for the current Windows
//! user. The program it installs (headway.exe) travels inside this one,
//! compressed. Its screens are `ui/setup.html`; the work is in `headway-install`.
//!
//! Nothing here needs administrator rights, and nothing is downloaded.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use headway_install as install;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};

/// headway.exe, gzip-compressed by build.rs. Empty in a build made without it.
static PACKED: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.gz"));

/// Shown as the "more information" link in Windows' Installed apps.
const ABOUT_URL: &str = "https://github.com/shinnahu/headway";

/// A gzip file ends with the size of what it holds, which is all we need to
/// show how much room the program takes.
fn program_bytes() -> u64 {
    match PACKED.len() {
        n if n >= 4 => u64::from(u32::from_le_bytes([PACKED[n - 4], PACKED[n - 3], PACKED[n - 2], PACKED[n - 1]])),
        _ => 0,
    }
}

/// What is left to delete once the setup has ended.
#[derive(Default)]
struct Later {
    files: Vec<PathBuf>,
    empty_dirs: Vec<PathBuf>,
}

struct State {
    later: Mutex<Later>,
    /// Where this window's browser engine keeps its files (a temporary folder).
    web_data: PathBuf,
    /// An installation is under way: the window stays open until it is done.
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

/// An error the window can show: `{ code, message }`.
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
struct Info {
    version: &'static str,
    /// Size of the program once installed; 0 when this setup has none inside.
    program_bytes: u64,
    /// Where Headway goes unless the user picks another folder: where it
    /// already is, for an update.
    folder: String,
    /// What Windows has on record about an existing installation.
    installed: Option<install::Installed>,
    /// The language the app is already set to, if one was chosen.
    language: Option<String>,
}

#[tauri::command]
fn setup_info() -> Result<Info, Failure> {
    let layout = install::Layout::standard()?;
    let installed = install::installed(&layout.app_id);
    let folder = installed.as_ref().map(|i| i.location.trim()).filter(|l| !l.is_empty()).map(PathBuf::from).unwrap_or(layout.install_dir);
    Ok(Info {
        version: env!("CARGO_PKG_VERSION"),
        program_bytes: program_bytes(),
        folder: folder.to_string_lossy().into_owned(),
        installed,
        language: install::config_file().and_then(|f| install::read_language(&f)),
    })
}

/// The folder Headway would go in if the user picks `picked`.
#[tauri::command]
fn setup_folder(picked: String) -> String {
    install::folder_for(Path::new(picked.trim())).to_string_lossy().into_owned()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    folder: String,
    desktop_shortcut: bool,
    start_menu_shortcut: bool,
    /// "en" or "es" when the user chose a language in the setup; otherwise the
    /// app keeps following the language of Windows.
    language: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    was_running: bool,
}

fn unpack() -> install::Result<Vec<u8>> {
    let mut program = Vec::with_capacity(program_bytes() as usize);
    if !PACKED.is_empty() {
        flate2::read::GzDecoder::new(PACKED)
            .read_to_end(&mut program)
            .map_err(|e| install::Error::Failed(format!("Could not read the program inside this setup: {e}")))?;
    }
    Ok(program)
}

#[tauri::command]
async fn setup_install(window: tauri::Window, state: tauri::State<'_, State>, request: Request) -> Result<Done, Failure> {
    let _busy = Busy::start(&state.busy);
    let work = move || -> install::Result<(Done, install::Removal)> {
        // The window shows each part as it starts.
        let say = |step: &str| {
            let _ = window.emit("setup-step", step);
        };
        say("unpack");
        let program = unpack()?;
        let folder = PathBuf::from(request.folder.trim());
        if !folder.is_absolute() {
            return Err(install::Error::Failed("Choose a folder to install Headway in.".into()));
        }
        let mut layout = install::Layout::standard()?;
        let before = install::installed(&layout.app_id);
        layout.install_dir = folder;
        let opts = install::Options {
            version: env!("CARGO_PKG_VERSION").into(),
            desktop_shortcut: request.desktop_shortcut,
            start_menu_shortcut: request.start_menu_shortcut,
            about_url: Some(ABOUT_URL.into()),
        };
        let outcome = install::install(&layout, &program, &opts, &mut |step| {
            say(match step {
                install::Step::Copy => "copy",
                install::Step::Shortcuts => "shortcuts",
                install::Step::Register => "register",
            })
        })?;

        // Installed somewhere else than before: the old copy is not needed any more.
        let mut left = install::Removal::default();
        let old = before.map(|b| PathBuf::from(b.location.trim())).filter(|old| !old.as_os_str().is_empty() && !install::same_folder(old, &layout.install_dir));
        if let Some(old) = old {
            left = install::remove_program(&old, &layout.exe_name).unwrap_or_default();
        }
        if let (Some(language), Some(file)) = (request.language.filter(|l| l == "en" || l == "es"), install::config_file()) {
            // Not worth failing an installation over: the app then follows Windows.
            let _ = install::write_language(&file, &language);
        }
        say("done");
        Ok((Done { was_running: outcome.was_running }, left))
    };
    let (done, left) = tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| Failure { code: "failed", message: e.to_string() })??;
    let mut later = state.later.lock().unwrap_or_else(|e| e.into_inner());
    later.files.extend(left.files);
    later.empty_dirs.extend(left.empty_dirs);
    Ok(done)
}

/// Starts the installed Headway (the "Open Headway now" box).
#[tauri::command]
fn setup_launch() -> Result<(), Failure> {
    let exe = install::Layout::of_existing()?.exe();
    let mut command = std::process::Command::new(&exe);
    if let Some(dir) = exe.parent() {
        command.current_dir(dir);
    }
    command.spawn().map(|_| ()).map_err(|e| {
        // i18n
        let message = format!("Could not open Headway: {e}");
        Failure { code: "failed", message }
    })
}

/// A plain Windows message box, for the one case where the setup's own window
/// cannot be drawn.
#[cfg(windows)]
fn message_box(title: &str, text: &str) {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "user32")]
    unsafe extern "system" {
        fn MessageBoxW(window: *mut core::ffi::c_void, text: *const u16, caption: *const u16, kind: u32) -> i32;
    }
    let wide = |s: &str| std::ffi::OsStr::new(s).encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    let (text, title) = (wide(text), wide(title));
    const MB_ICONINFORMATION: u32 = 0x40;
    // SAFETY: both strings are NUL-terminated UTF-16 that outlive the call.
    unsafe {
        MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), MB_ICONINFORMATION);
    }
}

#[cfg(not(windows))]
fn message_box(title: &str, text: &str) {
    eprintln!("{title}: {text}");
}

fn main() {
    // Headway's windows (this one included) are drawn by Microsoft's WebView2
    // runtime. Windows 11 has it; a Windows 10 that never received it does not.
    if !install::webview2_present() {
        message_box(
            "Headway Setup",
            "Headway needs Microsoft Edge WebView2 Runtime, which is not on this computer.\n\nInstall it from https://developer.microsoft.com/microsoft-edge/webview2/ (\"Evergreen Bootstrapper\") and run this setup again.\n\n\
             Headway necesita Microsoft Edge WebView2 Runtime, que no está en esta computadora.\n\nInstálalo desde la misma dirección y vuelve a ejecutar este instalador.",
        );
        return;
    }

    let web_data = std::env::temp_dir().join(format!("Headway-setup-{}", std::process::id()));
    let for_window = web_data.clone();
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(State { later: Mutex::new(Later::default()), web_data, busy: AtomicBool::new(false) })
        .invoke_handler(tauri::generate_handler![setup_info, setup_folder, setup_install, setup_launch])
        .setup(move |app| {
            WebviewWindowBuilder::new(app, "setup", WebviewUrl::App("setup.html".into()))
                .title("Headway Setup")
                .inner_size(880.0, 560.0)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .shadow(true)
                .center()
                .data_directory(for_window)
                .build()?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Headway Setup could not start");

    app.run(|handle, event| match event {
        // Closing the window in the middle of an installation would leave it half done.
        RunEvent::WindowEvent { event: tauri::WindowEvent::CloseRequested { api, .. }, .. } => {
            if handle.state::<State>().busy.load(Ordering::SeqCst) {
                api.prevent_close();
            }
        }
        RunEvent::Exit => {
            let state = handle.state::<State>();
            let later = state.later.lock().unwrap_or_else(|e| e.into_inner());
            let _ = install::delete_after_exit(&later.files, std::slice::from_ref(&state.web_data), &later.empty_dirs);
        }
        _ => {}
    });
}
