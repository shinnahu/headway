//! Installing and removing Headway on Windows, for the current user only (no
//! administrator rights are needed): the program file, the shortcuts, and the
//! entry in Windows' "Installed apps".
//!
//! Removal only deletes what installation put there. It never deletes a folder
//! with other things in it.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The program file is held by something that will not let go of it.
    #[error("Headway is running. Close it and try again.")]
    InUse,
    #[error("{0}")]
    Failed(String),
}

impl Error {
    pub fn code(&self) -> &'static str {
        match self {
            Error::InUse => "in_use",
            Error::Failed(_) => "failed",
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

fn failed(what: &str) -> impl Fn(std::io::Error) -> Error + '_ {
    move |e| Error::Failed(format!("Could not {what}: {e}"))
}

/// Windows reports a file that is open elsewhere as "access denied" (5) or
/// "sharing violation" (32).
fn locked(e: &std::io::Error) -> bool {
    matches!(e.raw_os_error(), Some(5) | Some(32))
}

/// The names and places of one installation. The real one is
/// [`Layout::standard`]; tests build their own so they never touch it.
#[derive(Clone, Debug)]
pub struct Layout {
    /// Key under Windows' uninstall list.
    pub app_id: String,
    /// Shown in "Installed apps" and used as the shortcut name.
    pub display_name: String,
    pub exe_name: String,
    pub install_dir: PathBuf,
    pub start_menu_dir: PathBuf,
    pub desktop_dir: PathBuf,
}

/// For trying the setup and the uninstaller for real without touching the
/// real installation: with the environment variable `HEADWAY_INSTALL_SANDBOX`
/// set to a folder, everything (program, shortcuts) goes inside that folder
/// and the entry in Installed apps gets a name of its own.
pub fn sandbox() -> Option<PathBuf> {
    std::env::var_os("HEADWAY_INSTALL_SANDBOX").filter(|v| !v.is_empty()).map(PathBuf::from)
}

impl Layout {
    pub fn standard() -> Result<Layout> {
        if let Some(root) = sandbox() {
            return Ok(Layout {
                app_id: "HeadwaySandbox".into(),
                display_name: "Headway (sandbox)".into(),
                exe_name: "headway.exe".into(),
                install_dir: root.join("Programs").join("Headway"),
                start_menu_dir: root.join("Start Menu"),
                desktop_dir: root.join("Desktop"),
            });
        }
        let missing = || Error::Failed("Windows did not say where your user folders are.".into());
        let local = dirs::data_local_dir().ok_or_else(missing)?;
        let roaming = dirs::config_dir().ok_or_else(missing)?;
        let desktop = dirs::desktop_dir().or_else(|| dirs::home_dir().map(|h| h.join("Desktop"))).ok_or_else(missing)?;
        Ok(Layout {
            app_id: "Headway".into(),
            display_name: "Headway".into(),
            exe_name: "headway.exe".into(),
            install_dir: local.join("Programs").join("Headway"),
            start_menu_dir: roaming.join("Microsoft").join("Windows").join("Start Menu").join("Programs"),
            desktop_dir: desktop,
        })
    }

    /// The standard layout, pointed at the folder an existing installation is in.
    pub fn of_existing() -> Result<Layout> {
        let mut layout = Layout::standard()?;
        if let Some(found) = installed(&layout.app_id) {
            if !found.location.trim().is_empty() {
                layout.install_dir = PathBuf::from(found.location);
            }
        }
        Ok(layout)
    }

    pub fn exe(&self) -> PathBuf {
        self.install_dir.join(&self.exe_name)
    }

    pub fn start_menu_link(&self) -> PathBuf {
        self.start_menu_dir.join(format!("{}.lnk", self.display_name))
    }

    pub fn desktop_link(&self) -> PathBuf {
        self.desktop_dir.join(format!("{}.lnk", self.display_name))
    }
}

/// The folder Headway goes in when the user picks `picked`: a folder of its
/// own inside it, unless they already chose one called Headway.
pub fn folder_for(picked: &Path) -> PathBuf {
    let own = picked.file_name().is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("Headway"));
    if own { picked.to_path_buf() } else { picked.join("Headway") }
}

pub struct Options {
    pub version: String,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
    /// Shown as the "more information" link in Installed apps.
    pub about_url: Option<String>,
}

/// What Windows has on record about an existing installation.
#[derive(Clone, Debug, Default, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Installed {
    pub version: String,
    pub location: String,
}

/// The parts of an installation, in the order they happen.
#[derive(Clone, Copy, Debug, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Step {
    Copy,
    Shortcuts,
    Register,
}

#[derive(Debug)]
pub struct Outcome {
    /// The installed program.
    pub exe: PathBuf,
    /// An older copy was running while it was replaced. That copy keeps
    /// working until it is closed; the new one is used from the next start.
    pub was_running: bool,
}

/// Puts the program in place, creates the shortcuts and registers it.
/// `progress` hears about each part as it starts.
pub fn install(layout: &Layout, program: &[u8], opts: &Options, progress: &mut dyn FnMut(Step)) -> Result<Outcome> {
    if program.is_empty() {
        return Err(Error::Failed("This setup was built without the program inside.".into()));
    }
    progress(Step::Copy);
    std::fs::create_dir_all(&layout.install_dir).map_err(failed("create the folder"))?;
    let exe = layout.exe();
    sweep(&layout.install_dir, &layout.exe_name);
    // Write next to the target first, then swap: a half-written program never
    // replaces a working one.
    let fresh = sibling(&exe, "new");
    std::fs::write(&fresh, program).map_err(failed("write the program"))?;
    let was_running = match put_in_place(&fresh, &exe) {
        Ok(was_running) => was_running,
        Err(e) => {
            let _ = std::fs::remove_file(&fresh);
            return Err(e);
        }
    };

    progress(Step::Shortcuts);
    let _ = std::fs::remove_file(layout.start_menu_link());
    let _ = std::fs::remove_file(layout.desktop_link());
    if opts.start_menu_shortcut {
        shortcut(&exe, &layout.start_menu_link(), &layout.display_name)?;
    }
    if opts.desktop_shortcut {
        shortcut(&exe, &layout.desktop_link(), &layout.display_name)?;
    }

    progress(Step::Register);
    register(layout, opts, program.len())?;
    Ok(Outcome { exe, was_running })
}

/// "headway.exe" -> "headway.exe.new"
fn sibling(exe: &Path, suffix: &str) -> PathBuf {
    let mut name = exe.file_name().unwrap_or_default().to_os_string();
    name.push(".");
    name.push(suffix);
    exe.with_file_name(name)
}

/// Moves the fresh program over the installed one. Windows will not replace a
/// program that is running (Headway's window, or the copy Claude starts for
/// MCP), but it lets it be renamed: the old one is set aside under another
/// name and deleted later, once nothing uses it. Returns whether that was needed.
fn put_in_place(fresh: &Path, exe: &Path) -> Result<bool> {
    match std::fs::rename(fresh, exe) {
        Ok(()) => Ok(false),
        Err(e) if locked(&e) => {
            let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
            let aside = sibling(exe, &format!("old-{stamp}"));
            std::fs::rename(exe, &aside).map_err(|e| if locked(&e) { Error::InUse } else { failed("set the old program aside")(e) })?;
            match std::fs::rename(fresh, exe) {
                Ok(()) => {
                    let _ = std::fs::remove_file(&aside);
                    Ok(true)
                }
                Err(e) => {
                    // Put the old one back so the installation is as it was.
                    let _ = std::fs::rename(&aside, exe);
                    Err(failed("put the program in place")(e))
                }
            }
        }
        Err(e) => Err(failed("put the program in place")(e)),
    }
}

/// Files a previous update left next to the program: a half-written new one,
/// or old ones that were still running then. Deletes the ones it can.
pub fn sweep(dir: &Path, exe_name: &str) {
    for path in leftovers(dir, exe_name) {
        let _ = std::fs::remove_file(path);
    }
}

fn leftovers(dir: &Path, exe_name: &str) -> Vec<PathBuf> {
    let new = format!("{exe_name}.new").to_lowercase();
    let old = format!("{exe_name}.old-").to_lowercase();
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .filter(|e| {
            let name = e.file_name().to_string_lossy().to_lowercase();
            name == new || name.starts_with(&old)
        })
        .map(|e| e.path())
        .collect()
}

/// [`sweep`] for the program that is running, at its start. Does nothing for
/// a copy that is not called headway.exe.
pub fn sweep_beside_running_program() {
    let Ok(exe) = std::env::current_exe() else { return };
    let (Some(dir), Some(name)) = (exe.parent(), exe.file_name()) else { return };
    let name = name.to_string_lossy();
    if name.eq_ignore_ascii_case("headway.exe") {
        sweep(dir, &name);
    }
}

/// What [`uninstall`] could not delete yet because it is still in use (the
/// program that is doing the uninstalling, for one). Hand it to
/// [`delete_after_exit`] when the program ends.
#[derive(Debug, Default)]
pub struct Removal {
    pub files: Vec<PathBuf>,
    /// Folders to remove afterwards, only if they are empty by then.
    pub empty_dirs: Vec<PathBuf>,
}

impl Removal {
    pub fn is_complete(&self) -> bool {
        self.files.is_empty() && self.empty_dirs.is_empty()
    }
}

/// Removes the shortcuts, the Installed apps entry and the program file. The
/// folder itself goes only if nothing else is left in it.
///
/// Copies of Headway that are still running from that folder (its window, or
/// the MCP server another program started) are ended first: a program that
/// runs cannot be deleted. The one exception is the program doing the
/// uninstalling. Its file is moved out of the folder (to the temporary
/// folder), so the installation is gone at once, and reported in the result,
/// to be deleted once it has ended.
pub fn uninstall(layout: &Layout) -> Result<Removal> {
    end_other_copies(&layout.install_dir, &layout.exe_name);
    let _ = std::fs::remove_file(layout.start_menu_link());
    let _ = std::fs::remove_file(layout.desktop_link());
    unregister(&layout.app_id);
    remove_program(&layout.install_dir, &layout.exe_name)
}

/// Deletes the program file (and what updates left next to it) from `dir`,
/// then `dir` itself if nothing else is in it. Also used when an update moves
/// Headway to another folder.
pub fn remove_program(dir: &Path, exe_name: &str) -> Result<Removal> {
    static PARKED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let mut left = Removal::default();
    let mut programs = leftovers(dir, exe_name);
    programs.push(dir.join(exe_name));
    let me = std::env::current_exe().ok();
    for file in &programs {
        // A copy that was just ended lets go of its file a moment later. The
        // file of this very program never comes free, so it gets one try.
        let tries = if me.as_deref().is_some_and(|me| tidy(me) == tidy(file)) { 1 } else { 15 };
        match remove_when_free(file, tries) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) if locked(&e) => {
                let n = PARKED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                let parked = std::env::temp_dir().join(format!("Headway-removed-{}-{n}.exe", std::process::id()));
                // Moving only works within one drive; otherwise the file waits where it is.
                left.files.push(if std::fs::rename(file, &parked).is_ok() { parked } else { file.clone() });
            }
            Err(e) => return Err(failed("delete the program")(e)),
        }
    }
    // Fails, on purpose, when the folder still holds something we did not put there.
    if std::fs::remove_dir(dir).is_err() && left.files.iter().any(|f| f.parent() == Some(dir)) {
        left.empty_dirs.push(dir.to_path_buf());
    }
    Ok(left)
}

fn remove_when_free(file: &Path, tries: u32) -> std::io::Result<()> {
    let mut left = tries;
    loop {
        match std::fs::remove_file(file) {
            Err(e) if locked(&e) && left > 1 => {
                left -= 1;
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            other => return other,
        }
    }
}

/// A path in one spelling, for comparing: full, long names, backslashes, lower case.
fn tidy(path: &Path) -> String {
    let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    full.to_string_lossy().trim_start_matches(r"\\?\").replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Whether two folder paths are the same place, the way Windows compares them.
pub fn same_folder(a: &Path, b: &Path) -> bool {
    tidy(a) == tidy(b)
}

/// Ends every other running copy of the program that lives in `dir`: Headway's
/// window, or the MCP server another program started. Returns how many.
///
/// It never touches the program that calls it, nor a program of the same name
/// that runs from another folder.
#[cfg(windows)]
pub fn end_other_copies(dir: &Path, exe_name: &str) -> usize {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS};
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcessId, OpenProcess, QueryFullProcessImageNameW, TerminateProcess, WaitForSingleObject, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        PROCESS_SYNCHRONIZE, PROCESS_TERMINATE,
    };

    let wanted = exe_name.to_lowercase();
    let folder = tidy(dir);
    let mut ended = 0;
    // SAFETY: plain Win32 calls. Every handle is opened and closed here, the
    // entry is sized as the API asks, and the name buffer is passed with its length.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return 0;
        }
        let me = GetCurrentProcessId();
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snapshot, &mut entry) != 0;
        while more {
            let len = entry.szExeFile.iter().position(|&c| c == 0).unwrap_or(entry.szExeFile.len());
            // A process keeps the name it was started under, also after an
            // update renamed its file; where the file is now comes next.
            let name = String::from_utf16_lossy(&entry.szExeFile[..len]).to_lowercase();
            if entry.th32ProcessID != me && name == wanted {
                let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, entry.th32ProcessID);
                if !process.is_null() {
                    let mut buffer = [0u16; 1024];
                    let mut size = buffer.len() as u32;
                    if QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buffer.as_mut_ptr(), &mut size) != 0 {
                        let file = PathBuf::from(String::from_utf16_lossy(&buffer[..size as usize]));
                        if file.parent().is_some_and(|p| tidy(p) == folder) && TerminateProcess(process, 1) != 0 {
                            WaitForSingleObject(process, 5000);
                            ended += 1;
                        }
                    }
                    CloseHandle(process);
                }
            }
            more = Process32NextW(snapshot, &mut entry) != 0;
        }
        CloseHandle(snapshot);
    }
    ended
}

#[cfg(not(windows))]
pub fn end_other_copies(_: &Path, _: &str) -> usize {
    0
}

/// Deletes files and folders a few seconds from now, after this program has
/// ended and let go of them: `files`, then `trees` (folders with everything in
/// them), then `empty_dirs` (folders, only if empty).
///
/// A program cannot delete its own file while it runs, so the deleting is
/// handed to Windows' command interpreter, which waits, tries, waits and tries
/// once more.
#[cfg(windows)]
pub fn delete_after_exit(files: &[PathBuf], trees: &[PathBuf], empty_dirs: &[PathBuf]) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    if files.is_empty() && trees.is_empty() && empty_dirs.is_empty() {
        return Ok(());
    }
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let system = std::env::var_os("SystemRoot").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32");

    // The paths travel as environment variables and are only ever used inside
    // quotes, so nothing in a folder name can be read as a command.
    let mut vars: Vec<(String, &Path)> = Vec::new();
    let mut steps: Vec<String> = Vec::new();
    for (kind, command, list) in [("F", "del /f /q", files), ("T", "rmdir /s /q", trees), ("D", "rmdir", empty_dirs)] {
        for (i, path) in list.iter().enumerate() {
            let name = format!("HEADWAY_{kind}{i}");
            steps.push(format!("{command} \"%{name}%\" >nul 2>&1"));
            vars.push((name, path));
        }
    }
    let wait = |pings: u32| format!("\"{}\" -n {pings} 127.0.0.1 >nul 2>&1", system.join("PING.EXE").display());
    let pass = steps.join(" & ");
    let script = format!("{} & {pass} & {} & {pass}", wait(3), wait(6));

    let spawn = |flags: u32| {
        let mut cmd = Command::new(system.join("cmd.exe"));
        // /s: the text after /c is taken as it is, minus its outer quotes.
        cmd.raw_arg(format!("/d /s /c \"{script}\""));
        for (name, path) in &vars {
            cmd.env(name, path);
        }
        cmd.current_dir(&system).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).creation_flags(flags).spawn().map(|_| ())
    };
    // Outside any job this program was started in, where that is allowed, so
    // it is not stopped together with it.
    spawn(CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB).or_else(|_| spawn(CREATE_NO_WINDOW))
}

#[cfg(not(windows))]
pub fn delete_after_exit(_: &[PathBuf], _: &[PathBuf], _: &[PathBuf]) -> std::io::Result<()> {
    Ok(())
}

#[cfg(windows)]
fn shortcut(target: &Path, link: &Path, description: &str) -> Result<()> {
    let bad = |e: String| Error::Failed(format!("Could not create a shortcut: {e}"));
    if let Some(dir) = link.parent() {
        std::fs::create_dir_all(dir).map_err(|e| bad(e.to_string()))?;
    }
    let mut sl = mslnk::ShellLink::new(target).map_err(|e| bad(format!("{e:?}")))?;
    sl.set_name(Some(description.to_string()));
    sl.set_working_dir(target.parent().map(|p| p.to_string_lossy().to_string()));
    sl.create_lnk(link).map_err(|e| bad(format!("{e:?}")))
}

#[cfg(windows)]
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall";

#[cfg(windows)]
fn register(layout: &Layout, opts: &Options, bytes: usize) -> Result<()> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let bad = |e: std::io::Error| Error::Failed(format!("Could not add Headway to the list of installed apps: {e}"));
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"{UNINSTALL_KEY}\{}", layout.app_id)).map_err(bad)?;
    let exe = layout.exe().to_string_lossy().to_string();
    let text = |name: &str, value: String| key.set_value(name, &value).map_err(bad);
    text("DisplayName", layout.display_name.clone())?;
    text("DisplayVersion", opts.version.clone())?;
    text("Publisher", layout.display_name.clone())?;
    text("InstallLocation", layout.install_dir.to_string_lossy().to_string())?;
    text("DisplayIcon", format!("{exe},0"))?;
    text("UninstallString", format!("\"{exe}\" --uninstall"))?;
    if let Some(url) = &opts.about_url {
        text("URLInfoAbout", url.clone())?;
    }
    key.set_value("NoModify", &1u32).map_err(bad)?;
    key.set_value("NoRepair", &1u32).map_err(bad)?;
    key.set_value("EstimatedSize", &((bytes / 1024) as u32)).map_err(bad)?;
    Ok(())
}

#[cfg(windows)]
fn unregister(app_id: &str) {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(format!(r"{UNINSTALL_KEY}\{app_id}"));
}

/// The existing installation Windows knows about, if any.
#[cfg(windows)]
pub fn installed(app_id: &str) -> Option<Installed> {
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!(r"{UNINSTALL_KEY}\{app_id}")).ok()?;
    Some(Installed {
        version: key.get_value::<String, _>("DisplayVersion").unwrap_or_default(),
        location: key.get_value::<String, _>("InstallLocation").unwrap_or_default(),
    })
}

/// Whether the WebView2 runtime (which draws Headway's windows) is present.
/// These are the registry places Microsoft documents for it.
#[cfg(windows)]
pub fn webview2_present() -> bool {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;
    const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
    let version = |root, path: String| -> bool {
        RegKey::predef(root)
            .open_subkey(path)
            .and_then(|k| k.get_value::<String, _>("pv"))
            .map(|pv| !pv.trim().is_empty() && pv.trim() != "0.0.0.0")
            .unwrap_or(false)
    };
    version(HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\WOW6432Node\{CLIENT}"))
        || version(HKEY_LOCAL_MACHINE, format!(r"SOFTWARE\{CLIENT}"))
        || version(HKEY_CURRENT_USER, format!(r"Software\{CLIENT}"))
}

#[cfg(not(windows))]
fn shortcut(_: &Path, _: &Path, _: &str) -> Result<()> {
    Err(Error::Failed("Headway's setup only works on Windows.".into()))
}
#[cfg(not(windows))]
fn register(_: &Layout, _: &Options, _: usize) -> Result<()> {
    Err(Error::Failed("Headway's setup only works on Windows.".into()))
}
#[cfg(not(windows))]
fn unregister(_: &str) {}
#[cfg(not(windows))]
pub fn installed(_: &str) -> Option<Installed> {
    None
}
#[cfg(not(windows))]
pub fn webview2_present() -> bool {
    true
}

// ---- the language chosen during setup ----------------------------------------------

/// Headway's settings file (the same place the app itself looks, including
/// its `HEADWAY_CONFIG_DIR` override).
pub fn config_file() -> Option<PathBuf> {
    let dir = match std::env::var_os("HEADWAY_CONFIG_DIR").filter(|v| !v.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => dirs::config_dir()?.join("Headway"),
    };
    Some(dir.join("config.json"))
}

/// The language the app is set to ("en" / "es"), if any.
pub fn read_language(config: &Path) -> Option<String> {
    let v: Value = serde_json::from_str(&std::fs::read_to_string(config).ok()?).ok()?;
    v.pointer("/settings/language").and_then(Value::as_str).map(str::to_string)
}

/// Sets the app's language, leaving everything else in the settings file as it is.
pub fn write_language(config: &Path, language: &str) -> Result<()> {
    let mut root = std::fs::read_to_string(config).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).filter(Value::is_object).unwrap_or_else(|| json!({}));
    if !root.get("settings").is_some_and(Value::is_object) {
        root["settings"] = json!({});
    }
    root["settings"]["language"] = json!(language);
    if let Some(dir) = config.parent() {
        std::fs::create_dir_all(dir).map_err(failed("create the settings folder"))?;
    }
    let text = serde_json::to_string_pretty(&root).map_err(|e| Error::Failed(e.to_string()))?;
    std::fs::write(config, text).map_err(failed("save the language"))
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;
    use std::process::{Command, Stdio};

    /// An installation in a scratch folder, under its own name in the registry.
    fn scratch(name: &str) -> (Layout, PathBuf) {
        let root = std::env::temp_dir().join(format!("headway-install-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let layout = Layout {
            app_id: format!("HeadwayInstallTest-{}-{name}", std::process::id()),
            display_name: "Headway Test".into(),
            exe_name: "headway.exe".into(),
            install_dir: root.join("app"),
            start_menu_dir: root.join("menu"),
            desktop_dir: root.join("desktop"),
        };
        (layout, root)
    }

    fn opts(desktop: bool) -> Options {
        Options { version: "9.9.9".into(), desktop_shortcut: desktop, start_menu_shortcut: true, about_url: Some("https://example.com".into()) }
    }

    fn put(layout: &Layout, program: &[u8], desktop: bool) -> Result<Outcome> {
        install(layout, program, &opts(desktop), &mut |_| {})
    }

    #[test]
    fn installs_then_removes_everything_it_created() {
        let (layout, root) = scratch("cycle");
        let mut steps = Vec::new();
        let done = install(&layout, b"program v1", &opts(true), &mut |s| steps.push(s)).unwrap();
        assert_eq!(steps, [Step::Copy, Step::Shortcuts, Step::Register]);
        assert!(!done.was_running);
        assert_eq!(std::fs::read(&done.exe).unwrap(), b"program v1");
        assert!(layout.start_menu_link().exists() && layout.desktop_link().exists());
        assert_eq!(installed(&layout.app_id), Some(Installed { version: "9.9.9".into(), location: layout.install_dir.to_string_lossy().to_string() }));

        // Installing again updates in place; a shortcut that is no longer wanted goes away.
        put(&layout, b"program v2", false).unwrap();
        assert_eq!(std::fs::read(&done.exe).unwrap(), b"program v2");
        assert!(layout.start_menu_link().exists() && !layout.desktop_link().exists());

        assert!(uninstall(&layout).unwrap().is_complete());
        assert!(!done.exe.exists() && !layout.start_menu_link().exists() && !layout.install_dir.exists());
        assert_eq!(installed(&layout.app_id), None);
        let _ = std::fs::remove_dir_all(root);
    }

    /// The real thing: a program that is running gets replaced, and removed.
    #[test]
    fn a_running_program_is_replaced_and_can_be_removed() {
        let (layout, root) = scratch("running");
        // Windows' own command interpreter stands in for Headway: it is a real
        // program that can be kept running for as long as the test needs.
        let cmd = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32").join("cmd.exe");
        let exe = put(&layout, &std::fs::read(cmd).unwrap(), false).unwrap().exe;
        let run = || Command::new(&exe).args(["/d", "/c", "pause"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();

        let mut old = run();
        let done = put(&layout, b"program v2", false).unwrap();
        assert!(done.was_running, "the old copy was running and had to be set aside");
        assert_eq!(std::fs::read(&exe).unwrap(), b"program v2");
        assert_eq!(leftovers(&layout.install_dir, &layout.exe_name).len(), 1, "the old copy waits under another name");
        old.kill().unwrap();
        old.wait().unwrap();
        // The next update (or the next start of the app) clears it away.
        put(&layout, &std::fs::read(std::env::var_os("ComSpec").unwrap()).unwrap(), false).unwrap();
        assert!(leftovers(&layout.install_dir, &layout.exe_name).is_empty());

        // A file that is running cannot be deleted (as for the uninstaller's
        // own): it is moved out, so the folder is gone at once, and reported.
        let mut running = run();
        let left = remove_program(&layout.install_dir, &layout.exe_name).unwrap();
        assert!(!layout.install_dir.exists(), "the folder is gone although the program was running");
        assert_eq!(left.files.len(), 1);
        assert!(left.files[0].starts_with(std::env::temp_dir()) && left.files[0].exists());
        running.kill().unwrap();
        running.wait().unwrap();
        std::fs::remove_file(&left.files[0]).unwrap();
        assert!(uninstall(&layout).unwrap().is_complete());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn uninstalling_ends_the_copies_that_still_run_and_only_those() {
        let (layout, root) = scratch("ends");
        let cmd = std::fs::read(std::env::var_os("ComSpec").unwrap()).unwrap();
        let exe = put(&layout, &cmd, true).unwrap().exe;
        let run = |exe: &Path| Command::new(exe).args(["/d", "/c", "pause"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().unwrap();
        // Two copies run from the installation: one that an update set aside
        // while it ran, and one from the program as it is now.
        let mut first = run(&exe);
        assert!(put(&layout, &cmd, true).unwrap().was_running);
        let mut second = run(&exe);
        // A program of the same name in another folder is somebody else's.
        let elsewhere = root.join("elsewhere");
        std::fs::create_dir_all(&elsewhere).unwrap();
        std::fs::write(elsewhere.join("headway.exe"), &cmd).unwrap();
        let mut bystander = run(&elsewhere.join("headway.exe"));

        let left = uninstall(&layout).unwrap();
        assert!(left.is_complete(), "nothing is left for later: {left:?}");
        assert!(!layout.install_dir.exists() && !layout.start_menu_link().exists() && !layout.desktop_link().exists());
        assert!(installed(&layout.app_id).is_none());
        assert!(first.wait().is_ok() && second.wait().is_ok(), "both copies were ended");
        assert!(bystander.try_wait().unwrap().is_none(), "the one in another folder still runs");
        bystander.kill().unwrap();
        bystander.wait().unwrap();
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_that_cannot_be_moved_is_reported_not_broken() {
        let (layout, root) = scratch("busy");
        let exe = put(&layout, b"program v1", false).unwrap().exe;
        // Held by something that shares it for reading only (a scanner, say):
        // it can be neither replaced nor renamed.
        let held = std::fs::OpenOptions::new().read(true).share_mode(1).open(&exe).unwrap();
        assert_eq!(put(&layout, b"program v2", false).unwrap_err().code(), "in_use");
        assert_eq!(std::fs::read(&exe).unwrap(), b"program v1");
        assert!(leftovers(&layout.install_dir, &layout.exe_name).is_empty(), "no half-written file is left behind");
        assert!(layout.start_menu_link().exists() && installed(&layout.app_id).is_some());

        // Uninstalling still unregisters it; the file is reported as left for later.
        let left = uninstall(&layout).unwrap();
        assert_eq!(left.files, [exe.clone()]);
        assert_eq!(left.empty_dirs, [layout.install_dir.clone()]);
        assert!(installed(&layout.app_id).is_none() && !layout.start_menu_link().exists());
        drop(held);
        assert!(uninstall(&layout).unwrap().is_complete());
        assert!(!layout.install_dir.exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn other_files_in_the_folder_are_left_alone() {
        let (layout, root) = scratch("extra");
        put(&layout, b"program", false).unwrap();
        let mine = layout.install_dir.join("my-notes.txt");
        std::fs::write(&mine, "keep me").unwrap();
        assert!(uninstall(&layout).unwrap().is_complete());
        assert!(!layout.exe().exists());
        assert_eq!(std::fs::read_to_string(&mine).unwrap(), "keep me");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn an_empty_setup_installs_nothing() {
        let (layout, root) = scratch("empty");
        assert_eq!(put(&layout, b"", false).unwrap_err().code(), "failed");
        assert!(!layout.install_dir.exists());
        assert_eq!(installed(&layout.app_id), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn deleting_after_exit_removes_files_trees_and_empty_folders_only() {
        let root = std::env::temp_dir().join(format!("headway-install-later-{} & co", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (file, tree, empty, kept) = (root.join("gone 100%.exe"), root.join("tree"), root.join("empty"), root.join("kept"));
        for dir in [&tree.join("deep"), &empty, &kept] {
            std::fs::create_dir_all(dir).unwrap();
        }
        for f in [&file, &tree.join("deep").join("a.bin"), &kept.join("mine.txt")] {
            std::fs::write(f, "x").unwrap();
        }
        delete_after_exit(&[file.clone()], &[tree.clone()], &[empty.clone(), kept.clone()]).unwrap();
        let gone = || !file.exists() && !tree.exists() && !empty.exists();
        for _ in 0..80 {
            if gone() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(250));
        }
        assert!(gone(), "the file, the tree and the empty folder are deleted");
        assert!(kept.join("mine.txt").exists(), "a folder with something in it stays");
        // Let the second pass finish before clearing the scratch folder.
        std::thread::sleep(std::time::Duration::from_secs(7));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_folder_gets_its_own_headway_folder() {
        assert_eq!(folder_for(Path::new(r"D:\Apps")), PathBuf::from(r"D:\Apps\Headway"));
        assert_eq!(folder_for(Path::new(r"D:\Apps\headway")), PathBuf::from(r"D:\Apps\headway"));
        assert_eq!(folder_for(Path::new(r"D:\")), PathBuf::from(r"D:\Headway"));
    }

    #[test]
    fn the_language_is_written_without_touching_other_settings() {
        let dir = std::env::temp_dir().join(format!("headway-install-lang-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("config.json");
        assert_eq!(read_language(&file), None);
        write_language(&file, "es").unwrap();
        assert_eq!(read_language(&file).as_deref(), Some("es"));

        std::fs::write(&file, r#"{"projects":[{"id":"o/r"}],"settings":{"syncSeconds":60}}"#).unwrap();
        write_language(&file, "en").unwrap();
        let v: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(v["settings"]["language"], "en");
        assert_eq!(v["settings"]["syncSeconds"], 60);
        assert_eq!(v["projects"][0]["id"], "o/r");
        let _ = std::fs::remove_dir_all(dir);
    }
}
