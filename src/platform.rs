use anyhow::{bail, Context, Result};

/// The operating systems gump supports. Anything else is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Fedora,
    Arch,
    Macos,
}

impl Os {
    /// Key used for `[install.<key>]` in app manifests.
    pub fn key(self) -> &'static str {
        match self {
            Os::Fedora => "fedora",
            Os::Arch => "arch",
            Os::Macos => "macos",
        }
    }

    /// Key used for per-family settings targets (`target.linux` / `target.macos`).
    pub fn family(self) -> &'static str {
        match self {
            Os::Macos => "macos",
            Os::Fedora | Os::Arch => "linux",
        }
    }

    pub fn display(self) -> &'static str {
        match self {
            Os::Fedora => "Fedora",
            Os::Arch => "Arch Linux",
            Os::Macos => "macOS",
        }
    }

    fn from_key(s: &str) -> Option<Os> {
        match s {
            "fedora" => Some(Os::Fedora),
            "arch" => Some(Os::Arch),
            "macos" => Some(Os::Macos),
            _ => None,
        }
    }
}

/// Detect the running OS. `GUMP_OS=fedora|arch|macos` overrides detection (useful for
/// previewing the catalog for another machine with `gump list`).
pub fn detect() -> Result<Os> {
    if let Ok(v) = std::env::var("GUMP_OS") {
        return Os::from_key(&v).with_context(|| format!("GUMP_OS={v} is not one of fedora, arch, macos"));
    }
    match std::env::consts::OS {
        "macos" => Ok(Os::Macos),
        "linux" => {
            let text = std::fs::read_to_string("/etc/os-release")
                .or_else(|_| std::fs::read_to_string("/usr/lib/os-release"))
                .context("could not read /etc/os-release")?;
            parse_os_release(&text)
                .context("unsupported Linux distribution: gump supports Fedora and Arch (and their derivatives)")
        }
        other => bail!("unsupported operating system: {other}"),
    }
}

/// Map an os-release file to a supported OS using `ID` and `ID_LIKE`.
pub fn parse_os_release(text: &str) -> Option<Os> {
    let mut ids = Vec::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once('=') {
            if k.trim() == "ID" || k.trim() == "ID_LIKE" {
                let v = v.trim().trim_matches('"').trim_matches('\'');
                ids.extend(v.split_whitespace().map(str::to_lowercase));
            }
        }
    }
    if ids.iter().any(|i| i == "fedora") {
        Some(Os::Fedora)
    } else if ids.iter().any(|i| i == "arch") {
        Some(Os::Arch)
    } else {
        None
    }
}
