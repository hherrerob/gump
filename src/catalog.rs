//! The app catalog: categories and apps (with per-OS install recipes and settings files).
//! It is embedded into the binary at build time, or read from disk with `--catalog`.
//!
//! Layout:
//! ```text
//! catalog/
//!   categories.toml               [[category]] id/name, in display order
//!   <category>/<app-id>.toml      the app manifest
//!   <category>/<app-id>/<files>   settings sources referenced by the manifest (only for
//!                                 apps that have settings)
//! ```
//!
//! `<category>/<app-id>/app.toml` is also accepted, to keep a manifest next to its files.

use crate::backend::PkgKind;
use crate::platform::Os;
use anyhow::{bail, Context, Result};
use include_dir::{include_dir, Dir};
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Path, PathBuf};

static EMBEDDED: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/catalog");

/// Every file in the catalog, keyed by its `/`-separated path relative to the catalog root.
type Files = BTreeMap<String, Vec<u8>>;

#[derive(Debug, Clone)]
pub enum Source {
    Embedded,
    Disk(PathBuf),
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Source::Embedded => write!(f, "embedded"),
            Source::Disk(p) => write!(f, "{}", p.display()),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Category {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct CategoriesFile {
    #[serde(default)]
    category: Vec<Category>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    depends: Vec<String>,
    #[serde(default)]
    install: InstallMap,
    /// Extra packages that belong to the app's configuration (e.g. zsh plugins). They are treated
    /// like settings: installed even when the app already is, skipped with `--agnostic`.
    #[serde(default)]
    extras: InstallMap,
    check: Option<Check>,
    #[serde(default)]
    settings: Vec<SettingDef>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallMap {
    pub fedora: Option<InstallSpec>,
    pub arch: Option<InstallSpec>,
    pub macos: Option<InstallSpec>,
}

/// How to install an app on one OS. Steps run in order: `pre`, packages (dnf, pacman, aur,
/// flatpak, brew, cask), `script`, `post`. All of it is skipped when the app is already installed.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InstallSpec {
    #[serde(default)]
    pub pre: Vec<String>,
    #[serde(default)]
    pub dnf: Vec<String>,
    #[serde(default)]
    pub pacman: Vec<String>,
    #[serde(default)]
    pub aur: Vec<String>,
    #[serde(default)]
    pub flatpak: Vec<String>,
    #[serde(default)]
    pub brew: Vec<String>,
    #[serde(default)]
    pub cask: Vec<String>,
    /// VS Code extension ids; only allowed in `[extras.*]`.
    #[serde(default)]
    pub vscode: Vec<String>,
    pub script: Option<String>,
    #[serde(default)]
    pub post: Vec<String>,
}

impl InstallMap {
    pub fn get(&self, os: Os) -> Option<&InstallSpec> {
        match os {
            Os::Fedora => self.fedora.as_ref(),
            Os::Arch => self.arch.as_ref(),
            Os::Macos => self.macos.as_ref(),
        }
    }
}

impl InstallSpec {
    /// Package lists by kind, in install order, skipping empty ones.
    pub fn package_groups(&self) -> Vec<(PkgKind, &[String])> {
        [
            (PkgKind::Dnf, &self.dnf),
            (PkgKind::Pacman, &self.pacman),
            (PkgKind::Aur, &self.aur),
            (PkgKind::Flatpak, &self.flatpak),
            (PkgKind::Brew, &self.brew),
            (PkgKind::Cask, &self.cask),
            (PkgKind::Vscode, &self.vscode),
        ]
        .into_iter()
        .filter(|(_, v)| !v.is_empty())
        .map(|(k, v)| (k, v.as_slice()))
        .collect()
    }
}

/// Extra "is it installed?" detection. Passes when ANY of the given conditions holds.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Check {
    /// A binary that exists on PATH.
    pub command: Option<String>,
    /// A file or directory that exists (supports `~` and `$XDG_*`).
    pub path: Option<String>,
    /// A shell snippet that exits 0.
    pub shell: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingDef {
    source: String,
    target: Target,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Target {
    Any(String),
    PerFamily(PerFamily),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PerFamily {
    pub linux: Option<String>,
    pub macos: Option<String>,
}

/// A settings source resolved against the catalog: one file, or every file under a directory.
#[derive(Debug, Clone)]
pub struct Setting {
    pub source: String,
    pub target: Target,
    /// `(path relative to source, contents)`. The relative path is empty for a single file.
    pub files: Vec<(String, Vec<u8>)>,
}

impl Setting {
    pub fn target_for(&self, os: Os) -> Option<&str> {
        match &self.target {
            Target::Any(t) => Some(t),
            Target::PerFamily(p) => match os.family() {
                "macos" => p.macos.as_deref(),
                _ => p.linux.as_deref(),
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct App {
    pub id: String,
    pub category: String,
    pub name: String,
    pub description: String,
    pub depends: Vec<String>,
    pub install: InstallMap,
    pub extras: InstallMap,
    pub check: Option<Check>,
    pub settings: Vec<Setting>,
}

impl App {
    pub fn spec(&self, os: Os) -> Option<&InstallSpec> {
        self.install.get(os)
    }

    /// Extra packages for this OS, if any (only meaningful when the app is available).
    pub fn extras(&self, os: Os) -> Option<&InstallSpec> {
        self.extras.get(os)
    }

    /// Whether the app ships anything settings-like (files or extra packages) on this OS.
    pub fn has_settings(&self, os: Os) -> bool {
        self.extras(os).is_some() || self.settings.iter().any(|s| s.target_for(os).is_some())
    }

    pub fn available(&self, os: Os) -> bool {
        self.spec(os).is_some()
    }
}

#[derive(Debug)]
pub struct Catalog {
    pub source: Source,
    pub categories: Vec<Category>,
    /// Sorted by category order, then name.
    pub apps: Vec<App>,
}

impl Catalog {
    pub fn load(path: Option<&Path>) -> Result<Catalog> {
        match path {
            None => {
                let mut files = Files::new();
                collect_embedded(&EMBEDDED, &mut files);
                Catalog::from_files(files, Source::Embedded)
            }
            Some(root) => {
                let mut files = Files::new();
                collect_disk(root, root, &mut files)
                    .with_context(|| format!("reading catalog at {}", root.display()))?;
                Catalog::from_files(files, Source::Disk(root.to_path_buf()))
            }
        }
    }

    pub fn app(&self, id: &str) -> Option<&App> {
        self.apps.iter().find(|a| a.id == id)
    }

    fn from_files(files: Files, source: Source) -> Result<Catalog> {
        let mut errors: Vec<String> = Vec::new();

        let categories: Vec<Category> = match files.get("categories.toml") {
            Some(b) => parse_toml::<CategoriesFile>("categories.toml", b)?.category,
            None => bail!("catalog ({source}) has no categories.toml"),
        };

        let mut apps: Vec<App> = Vec::new();
        for (path, bytes) in &files {
            let parts: Vec<&str> = path.split('/').collect();
            let (category, id) = match parts[..] {
                [c, f] if f.ends_with(".toml") && f.len() > 5 => (c, &f[..f.len() - 5]),
                [c, id, "app.toml"] => (c, id),
                _ => continue,
            };
            if apps.iter().any(|a| a.id == id && a.category == category) {
                errors.push(format!("{category}/{id}: defined twice ({id}.toml and {id}/app.toml); keep one"));
                continue;
            }
            let manifest: Manifest = match parse_toml(path, bytes) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(format!("{e:#}"));
                    continue;
                }
            };
            let mut settings = Vec::new();
            for def in manifest.settings {
                match resolve_setting(&files, category, id, &def) {
                    Ok(files) => settings.push(Setting { source: def.source, target: def.target, files }),
                    Err(e) => errors.push(format!("{path}: {e}")),
                }
            }
            apps.push(App {
                id: id.to_string(),
                category: category.to_string(),
                name: manifest.name,
                description: manifest.description,
                depends: manifest.depends,
                install: manifest.install,
                extras: manifest.extras,
                check: manifest.check,
                settings,
            });
        }

        errors.extend(validate(&categories, &apps));
        if !errors.is_empty() {
            bail!("invalid catalog ({source}):\n  - {}", errors.join("\n  - "));
        }

        let order = |c: &str| categories.iter().position(|x| x.id == c).unwrap_or(usize::MAX);
        apps.sort_by(|a, b| {
            order(&a.category).cmp(&order(&b.category)).then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        });

        Ok(Catalog { source, categories, apps })
    }
}

fn parse_toml<T: serde::de::DeserializeOwned>(path: &str, bytes: &[u8]) -> Result<T> {
    let text = std::str::from_utf8(bytes).with_context(|| format!("{path}: not valid UTF-8"))?;
    toml::from_str(text).with_context(|| format!("{path}: parse error"))
}

fn resolve_setting(files: &Files, category: &str, id: &str, def: &SettingDef) -> Result<Vec<(String, Vec<u8>)>> {
    let src = def.source.trim_matches('/');
    if src.is_empty() || src.split('/').any(|c| c == ".." || c == ".") {
        bail!("settings source '{}' must be a relative path inside the app directory", def.source);
    }
    let base = format!("{category}/{id}/{src}");
    if let Some(b) = files.get(&base) {
        return Ok(vec![(String::new(), b.clone())]);
    }
    let prefix = format!("{base}/");
    let found: Vec<_> = files
        .range(prefix.clone()..)
        .take_while(|(k, _)| k.starts_with(&prefix))
        .map(|(k, v)| (k[prefix.len()..].to_string(), v.clone()))
        .collect();
    if found.is_empty() {
        bail!("settings source '{}' not found", def.source);
    }
    Ok(found)
}

fn validate(categories: &[Category], apps: &[App]) -> Vec<String> {
    let mut errors = Vec::new();
    let cat_ids: BTreeSet<&str> = categories.iter().map(|c| c.id.as_str()).collect();
    if cat_ids.len() != categories.len() {
        errors.push("categories.toml: duplicate category id".into());
    }
    let mut seen = BTreeSet::new();
    for a in apps {
        let at = format!("{}/{}", a.category, a.id);
        if !seen.insert(a.id.as_str()) {
            errors.push(format!("{at}: app id '{}' is used more than once", a.id));
        }
        if !cat_ids.contains(a.category.as_str()) {
            errors.push(format!("{at}: category '{}' is not listed in categories.toml", a.category));
        }
        let specs = [(Os::Fedora, &a.install.fedora), (Os::Arch, &a.install.arch), (Os::Macos, &a.install.macos)];
        if specs.iter().all(|(_, s)| s.is_none()) {
            errors.push(format!("{at}: no [install.fedora|arch|macos] section"));
        }
        for (os, spec) in specs {
            let Some(spec) = spec else { continue };
            let groups = spec.package_groups();
            if os == Os::Arch && spec.script.is_some() {
                errors.push(format!("{at}: [install.arch] can't use a script; only pacman and aur (yay)"));
            }
            if groups.is_empty() && spec.script.is_none() {
                errors.push(format!("{at}: [install.{}] has no packages and no script", os.key()));
            }
            for (kind, _) in &groups {
                if *kind == PkgKind::Vscode {
                    errors.push(format!("{at}: VS Code extensions belong in [extras.{}], not [install.*]", os.key()));
                } else if !kind.supported_on(os) {
                    errors.push(format!("{at}: '{}' packages are not supported on {}", kind.key(), os.key()));
                }
            }
            if groups.is_empty() && a.check.is_none() {
                errors.push(format!(
                    "{at}: [install.{}] is script-only, so it needs a [check] to stay idempotent",
                    os.key()
                ));
            }
        }
        for os in [Os::Fedora, Os::Arch, Os::Macos] {
            let Some(x) = a.extras(os) else { continue };
            let k = os.key();
            if a.spec(os).is_none() {
                errors.push(format!("{at}: [extras.{k}] without an [install.{k}] section"));
            }
            if x.script.is_some() {
                errors.push(format!("{at}: [extras.{k}] can't have a script; use packages plus pre/post"));
            }
            let groups = x.package_groups();
            if groups.is_empty() {
                errors.push(format!("{at}: [extras.{k}] lists no packages"));
            }
            for (kind, _) in &groups {
                if !kind.supported_on(os) {
                    errors.push(format!("{at}: '{}' extras are not supported on {k}", kind.key()));
                }
            }
        }
        if let Some(c) = &a.check {
            if c.command.is_none() && c.path.is_none() && c.shell.is_none() {
                errors.push(format!("{at}: [check] is empty"));
            }
        }
        for d in &a.depends {
            if !apps.iter().any(|x| &x.id == d) {
                errors.push(format!("{at}: depends on unknown app '{d}'"));
            }
        }
        for s in &a.settings {
            if let Target::PerFamily(p) = &s.target {
                if p.linux.is_none() && p.macos.is_none() {
                    errors.push(format!("{at}: settings '{}' has no target", s.source));
                }
            }
        }
    }
    errors
}

fn collect_embedded(dir: &Dir, out: &mut Files) {
    for f in dir.files() {
        out.insert(normalize(f.path()), f.contents().to_vec());
    }
    for d in dir.dirs() {
        collect_embedded(d, out);
    }
}

fn collect_disk(root: &Path, dir: &Path, out: &mut Files) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name() == ".DS_Store" {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect_disk(root, &path, out)?;
        } else {
            let rel = path.strip_prefix(root).expect("walk stays under root");
            out.insert(normalize(rel), std::fs::read(&path)?);
        }
    }
    Ok(())
}

fn normalize(p: &Path) -> String {
    p.components().map(|c| c.as_os_str().to_string_lossy()).collect::<Vec<_>>().join("/")
}
