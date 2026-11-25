use std::io::Write;
use std::path::PathBuf;

fn main() {
    // The program this setup installs is the finished headway.exe, named by the
    // HEADWAY_PAYLOAD variable (scripts/release.ps1 sets it) and packed into
    // the setup here. Without the variable the setup still builds, with nothing
    // inside, so the workspace compiles and tests on its own; a setup built
    // that way says so instead of installing.
    println!("cargo:rerun-if-env-changed=HEADWAY_PAYLOAD");
    let packed = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR")).join("payload.gz");
    match std::env::var_os("HEADWAY_PAYLOAD").filter(|p| !p.is_empty()) {
        Some(path) => {
            let path = PathBuf::from(path);
            println!("cargo:rerun-if-changed={}", path.display());
            let program = std::fs::read(&path).unwrap_or_else(|e| panic!("HEADWAY_PAYLOAD: cannot read {}: {e}", path.display()));
            let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
            gz.write_all(&program).expect("compress the program");
            std::fs::write(&packed, gz.finish().expect("compress the program")).expect("write the packed program");
        }
        None => std::fs::write(&packed, b"").expect("write the packed program"),
    }

    // Same icon as the app. The manifest says the setup runs as the user who
    // started it: it installs for that user only and never asks for
    // administrator rights.
    let windows = tauri_build::WindowsAttributes::new()
        .window_icon_path("../../src-tauri/icons/icon.ico")
        .app_manifest(include_str!("setup.exe.manifest"));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows)).expect("tauri-build failed");
}
