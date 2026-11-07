//! Settings files: path expansion, state detection, and copy-with-backup.

use anyhow::{bail, Context, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

fn home() -> Result<PathBuf> {
    dirs::home_dir().context("could not determine home directory")
}

fn env_or_default(name: &str, home: &Path) -> Option<String> {
    if let Ok(v) = std::env::var(name) {
        if !v.is_empty() {
            return Some(v);
        }
    }
    let rel = match name {
        "HOME" => "",
        "XDG_CONFIG_HOME" => ".config",
        "XDG_DATA_HOME" => ".local/share",
        "XDG_STATE_HOME" => ".local/state",
        "XDG_CACHE_HOME" => ".cache",
        _ => return None,
    };
    Some(home.join(rel).to_string_lossy().trim_end_matches('/').to_string())
}

/// Expand `~`, `$VAR` and `${VAR}` (with XDG defaults) into an absolute path.
pub fn expand(input: &str) -> Result<PathBuf> {
    let home = home()?;
    let s = if input == "~" {
        home.to_string_lossy().into_owned()
    } else if let Some(rest) = input.strip_prefix("~/") {
        format!("{}/{rest}", home.display())
    } else {
        input.to_string()
    };

    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let braced = chars.peek() == Some(&'{');
        if braced {
            chars.next();
        }
        let mut name = String::new();
        while let Some(&n) = chars.peek() {
            if n.is_ascii_alphanumeric() || n == '_' {
                name.push(n);
                chars.next();
            } else {
                break;
            }
        }
        if braced && chars.next() != Some('}') {
            bail!("unterminated ${{ in path '{input}'");
        }
        if name.is_empty() {
            bail!("empty variable in path '{input}'");
        }
        out.push_str(&env_or_default(&name, &home).with_context(|| format!("${name} is not set (in '{input}')"))?);
    }

    let p = PathBuf::from(out);
    if !p.is_absolute() {
        bail!("settings target '{input}' must be absolute (start with ~, / or $VAR)");
    }
    Ok(p)
}

/// Show a path with the home directory abbreviated to `~`.
pub fn tilde(p: &Path) -> String {
    match home().ok().and_then(|h| p.strip_prefix(&h).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".into(),
        Some(rest) => format!("~/{}", rest.display()),
        None => p.display().to_string(),
    }
}

/// Directory for gump's own state (logs, sudo shim): `$XDG_STATE_HOME/gump`.
pub fn state_dir() -> Result<PathBuf> {
    Ok(expand("$XDG_STATE_HOME")?.join("gump"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileState {
    Missing,
    Identical,
    Differs,
}

pub fn file_state(target: &Path, content: &[u8]) -> FileState {
    match std::fs::read(target) {
        Ok(existing) if existing == content => FileState::Identical,
        Ok(_) => FileState::Differs,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && target.symlink_metadata().is_err() => FileState::Missing,
        // Exists but unreadable (a directory, a broken symlink, permissions): treat as user-owned.
        Err(_) => FileState::Differs,
    }
}

/// Write `content` to `target`, creating parent directories. With `backup`, an existing target
/// is first renamed to `<name>.gump-bak-<unix-time>`; the backup path is returned.
pub fn write_file(target: &Path, content: &[u8], backup: bool) -> Result<Option<PathBuf>> {
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut backed_up = None;
    if backup && target.symlink_metadata().is_ok() {
        let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let name = target.file_name().context("target has no file name")?.to_string_lossy();
        let bak = target.with_file_name(format!("{name}.gump-bak-{ts}"));
        std::fs::rename(target, &bak).with_context(|| format!("backing up {}", target.display()))?;
        backed_up = Some(bak);
    }
    std::fs::write(target, content).with_context(|| format!("writing {}", target.display()))?;
    Ok(backed_up)
}
