//! Running `git` and `gh`. Arguments always go as an array (never through a
//! shell), and on Windows no console window flashes up.

use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use tokio::process::Command;

use crate::{Error, Result};

pub struct Output {
    pub ok: bool,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// What to show a person when the command failed.
    pub fn message(&self) -> String {
        let err = self.stderr.trim();
        if !err.is_empty() { err.to_string() } else { self.stdout.trim().to_string() }
    }
}

/// Opens a web link in the default browser. Only http(s) is accepted, and the
/// link never passes through a shell (links come from issue text, which
/// anyone who can write to the repo controls).
pub fn open_url(url: &str) -> Result<()> {
    let web = url.starts_with("https://") || url.starts_with("http://");
    let clean = !url.chars().any(|c| c.is_control() || c.is_whitespace() || c == '"');
    if !web || !clean {
        return Err(Error::invalid("Only web links can be opened."));
    }
    let mut cmd;
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd = std::process::Command::new("rundll32.exe");
        cmd.args(["url.dll,FileProtocolHandler", url]);
        cmd.creation_flags(0x0800_0000);
    }
    #[cfg(target_os = "macos")]
    {
        cmd = std::process::Command::new("open");
        cmd.arg(url);
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        cmd = std::process::Command::new("xdg-open");
        cmd.arg(url);
    }
    cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.spawn().map(|_| ()).map_err(|e| Error::Other(format!("Could not open the browser: {e}")))
}

pub async fn run(
    program: &str,
    args: &[&str],
    cwd: Option<&Path>,
    envs: &[(&str, &str)],
    timeout: Duration,
) -> Result<Output> {
    let mut cmd = Command::new(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    for (k, v) in envs {
        cmd.env(k, v);
    }
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW

    let out = match tokio::time::timeout(timeout, cmd.output()).await {
        Err(_) => {
            return Err(Error::Other(format!(
                "{program} did not finish within {} seconds.",
                timeout.as_secs()
            )));
        }
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::Other(format!("{program} is not installed (or not on PATH).")));
        }
        Ok(Err(e)) => return Err(Error::Io(format!("could not start {program}: {e}"))),
        Ok(Ok(out)) => out,
    };
    Ok(Output {
        ok: out.status.success(),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}
