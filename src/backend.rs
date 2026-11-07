//! Package-manager backends: "is this installed?" probes and install commands.

use crate::catalog::{App, Check, InstallSpec};
use crate::platform::Os;
use crate::settings;
use std::cell::OnceCell;
use std::collections::HashSet;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PkgKind {
    Dnf,
    Pacman,
    Aur,
    Flatpak,
    Brew,
    Cask,
    /// VS Code extensions (only valid in `[extras.*]`).
    Vscode,
}

impl PkgKind {
    pub fn key(self) -> &'static str {
        match self {
            PkgKind::Dnf => "dnf",
            PkgKind::Pacman => "pacman",
            PkgKind::Aur => "aur",
            PkgKind::Flatpak => "flatpak",
            PkgKind::Brew => "brew",
            PkgKind::Cask => "cask",
            PkgKind::Vscode => "vscode",
        }
    }

    pub fn supported_on(self, os: Os) -> bool {
        matches!(
            (self, os),
            (PkgKind::Dnf | PkgKind::Flatpak, Os::Fedora)
                | (PkgKind::Pacman | PkgKind::Aur, Os::Arch)
                | (PkgKind::Brew | PkgKind::Cask, Os::Macos)
                | (PkgKind::Vscode, _)
        )
    }
}

/// A command to run. A `sudo` program is run as `sudo -n` by the executor, so it never prompts.
#[derive(Clone, Debug)]
pub struct Cmd {
    pub program: String,
    pub args: Vec<String>,
    /// Whether this needs an authenticated sudo session (directly or via a child).
    pub privileged: bool,
    pub env: Vec<(String, String)>,
}

impl Cmd {
    pub fn new(program: impl Into<String>, args: &[&str]) -> Cmd {
        let program = program.into();
        Cmd {
            privileged: program == "sudo",
            program,
            args: args.iter().map(|s| s.to_string()).collect(),
            env: Vec::new(),
        }
    }

    pub fn sh(script: &str) -> Cmd {
        let mut c = Cmd::new("sh", &["-c", script]);
        c.privileged = script.contains("sudo");
        c
    }

    /// `sudo <args…>`.
    pub fn sudo(args: &[&str]) -> Cmd {
        Cmd::new("sudo", args)
    }

    pub fn privileged(mut self) -> Cmd {
        self.privileged = true;
        self
    }

    pub fn args(mut self, extra: &[String]) -> Cmd {
        self.args.extend(extra.iter().cloned());
        self
    }

    pub fn env(mut self, k: &str, v: &str) -> Cmd {
        self.env.push((k.into(), v.into()));
        self
    }

    pub fn display(&self) -> String {
        if self.program == "sh" && self.args.len() == 2 && self.args[0] == "-c" {
            return self.args[1].trim().to_string();
        }
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
    }
}

fn quiet(program: &str, args: &[&str]) -> bool {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn output_lines(program: &str, args: &[&str]) -> HashSet<String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect()
        })
        .unwrap_or_default()
}

pub fn brew_bin() -> String {
    if let Ok(p) = which::which("brew") {
        return p.to_string_lossy().into_owned();
    }
    for p in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
        if Path::new(p).exists() {
            return p.into();
        }
    }
    "brew".into()
}

pub fn has_brew() -> bool {
    brew_bin() != "brew"
}

pub fn has_yay() -> bool {
    which::which("yay").is_ok()
}

/// Caches the package manager's installed lists so a full scan only shells out once per backend.
#[derive(Default)]
pub struct Probe {
    rpm: OnceCell<HashSet<String>>,
    pacman: OnceCell<HashSet<String>>,
    flatpak: OnceCell<HashSet<String>>,
    brew: OnceCell<HashSet<String>>,
    cask: OnceCell<HashSet<String>>,
    vscode: OnceCell<HashSet<String>>,
}

impl Probe {
    pub fn installed(&self, kind: PkgKind, pkg: &str) -> bool {
        match kind {
            PkgKind::Dnf => {
                self.rpm.get_or_init(|| output_lines("rpm", &["-qa", "--qf", "%{NAME}\n"])).contains(pkg)
                    || quiet("rpm", &["-q", "--whatprovides", pkg])
            }
            PkgKind::Pacman | PkgKind::Aur => {
                self.pacman.get_or_init(|| output_lines("pacman", &["-Qq"])).contains(pkg)
                    || quiet("pacman", &["-Q", pkg])
                    || quiet("pacman", &["-Qg", pkg])
            }
            PkgKind::Flatpak => {
                self.flatpak.get_or_init(|| output_lines("flatpak", &["list", "--columns=application"])).contains(pkg)
            }
            PkgKind::Brew => {
                let short = pkg.rsplit('/').next().unwrap_or(pkg);
                self.brew.get_or_init(|| output_lines(&brew_bin(), &["list", "--formula", "-1"])).contains(short)
            }
            PkgKind::Cask => {
                let short = pkg.rsplit('/').next().unwrap_or(pkg);
                self.cask.get_or_init(|| output_lines(&brew_bin(), &["list", "--cask", "-1"])).contains(short)
            }
            // Extension ids are case-insensitive.
            PkgKind::Vscode => self
                .vscode
                .get_or_init(|| {
                    output_lines("code", &["--list-extensions"]).into_iter().map(|l| l.to_lowercase()).collect()
                })
                .contains(&pkg.to_lowercase()),
        }
    }
}

pub fn check_passes(c: &Check) -> bool {
    c.command.as_deref().is_some_and(|cmd| which::which(cmd).is_ok())
        || c.path.as_deref().is_some_and(|p| settings::expand(p).map(|p| p.exists()).unwrap_or(false))
        || c.shell.as_deref().is_some_and(|s| quiet("sh", &["-c", s]))
}

/// An app counts as installed when its `[check]` passes, or every package it declares for
/// this OS is installed.
pub fn app_installed(probe: &Probe, app: &App, os: Os) -> bool {
    if app.check.as_ref().is_some_and(check_passes) {
        return true;
    }
    let Some(spec) = app.spec(os) else { return false };
    let groups = spec.package_groups();
    !groups.is_empty() && groups.iter().all(|(k, pkgs)| pkgs.iter().all(|p| probe.installed(*k, p)))
}

/// Extra packages that are not installed yet, grouped by kind (empty when all are present).
pub fn missing_packages(probe: &Probe, spec: &InstallSpec) -> Vec<(PkgKind, Vec<String>)> {
    spec.package_groups()
        .into_iter()
        .map(|(k, pkgs)| (k, pkgs.iter().filter(|p| !probe.installed(k, p)).cloned().collect::<Vec<_>>()))
        .filter(|(_, v)| !v.is_empty())
        .collect()
}

pub fn install_cmd(kind: PkgKind, pkgs: &[String]) -> Cmd {
    match kind {
        PkgKind::Dnf => Cmd::sudo(&["dnf", "install", "-y"]).args(pkgs),
        PkgKind::Pacman => Cmd::sudo(&["pacman", "-S", "--needed", "--noconfirm"]).args(pkgs),
        PkgKind::Aur => Cmd::new("yay", &["-S", "--needed", "--noconfirm"])
            .privileged()
            .args(pkgs),
        PkgKind::Flatpak => Cmd::sudo(&["flatpak", "install", "-y", "--noninteractive", "flathub"]).args(pkgs),
        PkgKind::Brew => brew_cmd(&["install"]).args(pkgs),
        PkgKind::Cask => brew_cmd(&["install", "--cask"]).privileged().args(pkgs),
        PkgKind::Vscode => {
            let args: Vec<String> = pkgs.iter().flat_map(|p| ["--install-extension".into(), p.clone()]).collect();
            Cmd::new("code", &[]).args(&args)
        }
    }
}

fn brew_cmd(args: &[&str]) -> Cmd {
    Cmd::new(brew_bin(), args).env("NONINTERACTIVE", "1").env("HOMEBREW_NO_ENV_HINTS", "1")
}

/// All commands for installing an app from its spec, in order.
pub fn spec_cmds(spec: &InstallSpec) -> Vec<Cmd> {
    let mut cmds: Vec<Cmd> = spec.pre.iter().map(|s| Cmd::sh(s)).collect();
    cmds.extend(spec.package_groups().into_iter().map(|(k, pkgs)| install_cmd(k, pkgs)));
    if let Some(s) = &spec.script {
        cmds.push(Cmd::sh(s));
    }
    cmds.extend(spec.post.iter().map(|s| Cmd::sh(s)));
    cmds
}
