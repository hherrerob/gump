//! Turn a selection of apps into an ordered, idempotent plan of work.

use crate::backend::{self, Cmd, PkgKind, Probe};
use crate::catalog::{App, Catalog};
use crate::platform::Os;
use crate::settings::{self, FileState};
use anyhow::{bail, Result};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Replace existing settings files (after backing them up).
    pub override_settings: bool,
    /// Install apps only; never touch settings.
    pub agnostic: bool,
    /// Print what would happen without doing it.
    pub dry_run: bool,
    /// Show the plan for approval and the commands' output.
    pub verbose: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    Copy,
    Override,
    SkipIdentical,
    SkipExisting,
}

#[derive(Clone, Debug)]
pub struct FileOp {
    pub target: PathBuf,
    pub content: Vec<u8>,
    pub action: FileAction,
}

impl FileOp {
    pub fn writes(&self) -> bool {
        matches!(self.action, FileAction::Copy | FileAction::Override)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum UnitKind {
    #[default]
    Prereq,
    App {
        installed: bool,
        dependency: bool,
    },
}

/// One row of work: a prerequisite or an app.
#[derive(Clone, Debug, Default)]
pub struct Unit {
    pub id: String,
    pub label: String,
    pub kind: UnitKind,
    pub cmds: Vec<Cmd>,
    /// Commands installing missing extra packages (part of settings; empty with --agnostic).
    pub extras: Vec<Cmd>,
    /// Extra packages that are already installed (shown in the review).
    pub extras_present: Vec<String>,
    pub files: Vec<FileOp>,
    /// Unit ids that must succeed first (ids not in the plan count as satisfied).
    pub depends: Vec<String>,
}

impl Unit {
    pub fn is_noop(&self) -> bool {
        self.cmds.is_empty() && self.extras.is_empty() && !self.files.iter().any(FileOp::writes)
    }
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub units: Vec<Unit>,
    pub opts: Options,
}

impl Plan {
    pub fn is_noop(&self) -> bool {
        self.units.iter().all(Unit::is_noop)
    }

    pub fn needs_sudo(&self) -> bool {
        self.units.iter().flat_map(|u| u.cmds.iter().chain(&u.extras)).any(|c| c.privileged)
    }
}

/// Per-app status shown in the wizard and `gump status`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsBadge {
    None,
    Missing,
    Differs,
    Identical,
}

#[derive(Clone, Copy, Debug)]
pub struct AppStatus {
    pub available: bool,
    pub installed: bool,
    pub settings: SettingsBadge,
}

pub fn scan(cat: &Catalog, os: Os) -> Vec<AppStatus> {
    let probe = Probe::default();
    cat.apps
        .iter()
        .map(|app| {
            let available = app.available(os);
            let installed = available && backend::app_installed(&probe, app, os);
            let mut states: Vec<FileState> = setting_files(app, os)
                .into_iter()
                .filter_map(|r| r.ok())
                .map(|(t, c)| settings::file_state(&t, c))
                .collect();
            if let Some(x) = app.extras(os).filter(|_| available) {
                let missing = !backend::missing_packages(&probe, x).is_empty();
                states.push(if missing { FileState::Missing } else { FileState::Identical });
            }
            let settings = if states.is_empty() {
                SettingsBadge::None
            } else if states.contains(&FileState::Missing) {
                SettingsBadge::Missing
            } else if states.contains(&FileState::Differs) {
                SettingsBadge::Differs
            } else {
                SettingsBadge::Identical
            };
            AppStatus { available, installed, settings }
        })
        .collect()
}

/// Every (target path, content) pair an app would write on this OS.
fn setting_files(app: &App, os: Os) -> Vec<Result<(PathBuf, &[u8])>> {
    let mut out = Vec::new();
    for s in &app.settings {
        let Some(t) = s.target_for(os) else { continue };
        match settings::expand(t) {
            Ok(base) => {
                for (rel, content) in &s.files {
                    let target = if rel.is_empty() { base.clone() } else { base.join(rel) };
                    out.push(Ok((target, content.as_slice())));
                }
            }
            Err(e) => out.push(Err(e)),
        }
    }
    out
}

pub struct Resolved<'a> {
    /// Dependency-ordered apps; the flag is true for apps pulled in only as dependencies.
    pub apps: Vec<(&'a App, bool)>,
}

/// Look up the requested apps, pull in dependencies, and order everything so dependencies come first.
pub fn resolve<'a>(cat: &'a Catalog, targets: &[String], os: Os) -> Result<Resolved<'a>> {
    let mut requested: Vec<&App> = Vec::new();
    for t in targets {
        let Some(app) = cat.app(t) else { bail!("unknown app '{t}' (see `gump list`)") };
        if !app.available(os) {
            bail!("'{}' is not available on {}", app.id, os.display());
        }
        requested.push(app);
    }

    let wanted: HashSet<&str> = requested.iter().map(|a| a.id.as_str()).collect();
    let mut state: HashMap<&str, bool> = HashMap::new(); // false = visiting, true = done
    let mut order = Vec::new();

    fn visit<'a>(
        cat: &'a Catalog,
        app: &'a App,
        os: Os,
        state: &mut HashMap<&'a str, bool>,
        order: &mut Vec<&'a App>,
        chain: &mut Vec<&'a str>,
    ) -> Result<()> {
        match state.get(app.id.as_str()) {
            Some(true) => return Ok(()),
            Some(false) => bail!("dependency cycle: {} -> {}", chain.join(" -> "), app.id),
            None => {}
        }
        if !app.available(os) {
            bail!("'{}' (needed by {}) is not available on {}", app.id, chain.join(" -> "), os.display());
        }
        state.insert(&app.id, false);
        chain.push(&app.id);
        for d in &app.depends {
            visit(cat, cat.app(d).expect("validated"), os, state, order, chain)?;
        }
        chain.pop();
        state.insert(&app.id, true);
        order.push(app);
        Ok(())
    }

    for app in requested {
        visit(cat, app, os, &mut state, &mut order, &mut Vec::new())?;
    }
    Ok(Resolved { apps: order.into_iter().map(|a| (a, !wanted.contains(a.id.as_str()))).collect() })
}

fn prereq_id(kind: PkgKind) -> Option<&'static str> {
    match kind {
        PkgKind::Aur => Some("prereq:aur"),
        PkgKind::Flatpak => Some("prereq:flatpak"),
        PkgKind::Brew | PkgKind::Cask => Some("prereq:brew"),
        PkgKind::Dnf | PkgKind::Pacman | PkgKind::Vscode => None,
    }
}

const YAY_BOOTSTRAP: &str = r#"set -e
tmp=$(mktemp -d)
git clone --depth 1 https://aur.archlinux.org/yay-bin.git "$tmp/yay-bin"
cd "$tmp/yay-bin"
makepkg -si --noconfirm
rm -rf "$tmp""#;

const BREW_BOOTSTRAP: &str = r#"NONINTERACTIVE=1 /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)""#;

fn prereqs(kinds: &BTreeSet<PkgKind>) -> Vec<Unit> {
    let unit =
        |id: &str, label: &str, cmds: Vec<Cmd>| Unit { id: id.into(), label: label.into(), cmds, ..Unit::default() };
    let mut out = Vec::new();
    if kinds.contains(&PkgKind::Aur) && !backend::has_yay() {
        out.push(unit(
            "prereq:aur",
            "AUR helper (yay)",
            vec![
                backend::install_cmd(PkgKind::Pacman, &["base-devel".into(), "git".into()]),
                Cmd::sh(YAY_BOOTSTRAP).privileged(),
            ],
        ));
    }
    if kinds.contains(&PkgKind::Flatpak) {
        let mut cmds = Vec::new();
        if which::which("flatpak").is_err() {
            cmds.push(backend::install_cmd(PkgKind::Dnf, &["flatpak".into()]));
        }
        let has_flathub = std::process::Command::new("flatpak")
            // Installs are per-user (`flatpak --user`), so only a user-level flathub counts.
            .args(["remotes", "--user", "--columns=name"])
            .output()
            .is_ok_and(|o| String::from_utf8_lossy(&o.stdout).lines().any(|l| l.trim() == "flathub"));
        if !has_flathub {
            cmds.push(Cmd::new("flatpak", &[
                "remote-add",
                "--user",
                "--if-not-exists",
                "flathub",
                "https://dl.flathub.org/repo/flathub.flatpakrepo",
            ]));
        }
        if !cmds.is_empty() {
            out.push(unit("prereq:flatpak", "Flatpak + Flathub", cmds));
        }
    }
    if (kinds.contains(&PkgKind::Brew) || kinds.contains(&PkgKind::Cask)) && !backend::has_brew() {
        out.push(unit("prereq:brew", "Homebrew", vec![Cmd::sh(BREW_BOOTSTRAP).privileged()]));
    }
    out
}

pub fn build(cat: &Catalog, targets: &[String], os: Os, opts: Options) -> Result<Plan> {
    let resolved = resolve(cat, targets, os)?;
    let probe = Probe::default();
    let mut kinds = BTreeSet::new();
    let mut units = Vec::new();

    for (app, dependency) in resolved.apps {
        let spec = app.spec(os).expect("resolve only returns available apps");
        let installed = backend::app_installed(&probe, app, os);
        let mut depends = app.depends.clone();
        let cmds = if installed {
            Vec::new()
        } else {
            for (k, _) in spec.package_groups() {
                kinds.insert(k);
                depends.extend(prereq_id(k).map(String::from));
            }
            backend::spec_cmds(spec)
        };
        let mut extras = Vec::new();
        let mut extras_present = Vec::new();
        if let Some(x) = app.extras(os).filter(|_| !opts.agnostic) {
            let missing = backend::missing_packages(&probe, x);
            if missing.is_empty() {
                extras_present = x.package_groups().into_iter().flat_map(|(_, p)| p.iter().cloned()).collect();
            } else {
                extras.extend(x.pre.iter().map(|s| Cmd::sh(s)));
                for (k, pkgs) in &missing {
                    kinds.insert(*k);
                    depends.extend(prereq_id(*k).map(String::from));
                    extras.push(backend::install_cmd(*k, pkgs));
                }
                extras.extend(x.post.iter().map(|s| Cmd::sh(s)));
            }
        }
        let mut files = Vec::new();
        if !opts.agnostic {
            for entry in setting_files(app, os) {
                let (target, content) = entry?;
                let action = match (settings::file_state(&target, content), opts.override_settings) {
                    (FileState::Missing, _) => FileAction::Copy,
                    (FileState::Identical, _) => FileAction::SkipIdentical,
                    (FileState::Differs, true) => FileAction::Override,
                    (FileState::Differs, false) => FileAction::SkipExisting,
                };
                files.push(FileOp { target, content: content.to_vec(), action });
            }
        }
        depends.sort();
        depends.dedup();
        units.push(Unit {
            id: app.id.clone(),
            label: app.name.clone(),
            kind: UnitKind::App { installed, dependency },
            cmds,
            extras,
            extras_present,
            files,
            depends,
        });
    }

    let mut all = prereqs(&kinds);
    all.extend(units);
    Ok(Plan { units: all, opts })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Header,
    Action,
    Skip,
    Warn,
}

pub struct DescLine {
    pub tone: Tone,
    pub text: String,
}

/// Human-readable plan, shared by the wizard's review screen and `gump install`.
pub fn describe(plan: &Plan) -> Vec<DescLine> {
    let mut out = Vec::new();
    let mut line = |tone, text: String| out.push(DescLine { tone, text });
    for u in &plan.units {
        let suffix = match u.kind {
            UnitKind::Prereq => " (prerequisite)",
            UnitKind::App { dependency: true, .. } => " (dependency)",
            _ => "",
        };
        line(Tone::Header, format!("{}{suffix}", u.label));
        if let UnitKind::App { installed: true, .. } = u.kind {
            line(Tone::Skip, "  install   already installed".into());
        }
        let show = |c: &Cmd| {
            let shown = c.display();
            let more = if shown.lines().count() > 1 { " …" } else { "" };
            format!("{}{more}", shown.lines().next().unwrap_or_default())
        };
        for c in &u.cmds {
            line(Tone::Action, format!("  run       {}", show(c)));
        }
        for c in &u.extras {
            line(Tone::Action, format!("  extras    {}", show(c)));
        }
        if !u.extras_present.is_empty() {
            line(Tone::Skip, format!("  extras    installed   {}", u.extras_present.join(", ")));
        }
        for f in &u.files {
            let p = settings::tilde(&f.target);
            match f.action {
                FileAction::Copy => line(Tone::Action, format!("  settings  copy      {p}")),
                FileAction::Override => line(Tone::Warn, format!("  settings  override  {p} (old file backed up)")),
                FileAction::SkipIdentical => line(Tone::Skip, format!("  settings  current   {p}")),
                FileAction::SkipExisting => {
                    line(Tone::Skip, format!("  settings  keep      {p} (exists, differs; --override replaces)"))
                }
            }
        }
    }
    if plan.opts.agnostic {
        line(Tone::Skip, "Agnostic mode: settings files and extras are not touched.".into());
    }
    out
}
