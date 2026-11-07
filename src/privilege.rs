//! sudo handling: authenticate once up front, keep the ticket alive, and make sure no child
//! process can ever stop and prompt for a password (which would corrupt the TUI).

use crate::settings;
use anyhow::{bail, Context, Result};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Once;
use std::time::Duration;

pub fn ensure_not_root() -> Result<()> {
    // SAFETY: geteuid has no preconditions and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        bail!("run gump as your normal user, not root; it asks for sudo itself when needed");
    }
    Ok(())
}

pub fn real_sudo() -> String {
    which::which("sudo").map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "/usr/bin/sudo".into())
}

static KEEPALIVE: Once = Once::new();

/// Prompt for the sudo password (interactively, on the real terminal) and keep the ticket fresh.
pub fn acquire() -> Result<()> {
    println!("gump needs sudo to install packages.");
    let ok = Command::new(real_sudo()).arg("-v").status().context("running sudo")?.success();
    if !ok {
        bail!("sudo authentication failed");
    }
    KEEPALIVE.call_once(|| {
        let sudo = real_sudo();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(60));
            let _ = Command::new(&sudo)
                .args(["-n", "-v"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        });
    });
    Ok(())
}

/// A directory holding a `sudo` wrapper that always passes `-n`. It is put first on PATH for
/// every child, so scripts and tools like paru/makepkg fail instead of prompting.
pub fn shim_dir() -> Option<PathBuf> {
    let dir = settings::state_dir().ok()?.join("shim");
    let script = format!("#!/bin/sh\nexec {} -n \"$@\"\n", real_sudo());
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("sudo");
    std::fs::write(&path, script).ok()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).ok()?;
    }
    Some(dir)
}
