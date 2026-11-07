mod backend;
mod catalog;
mod executor;
mod plan;
mod platform;
mod privilege;
mod settings;
mod tui;

use anyhow::{bail, Result};
use catalog::Catalog;
use clap::{Args, Parser, Subcommand};
use executor::Event;
use plan::{Options, SettingsBadge, Tone};
use std::io::Write;
use std::path::PathBuf;

/// "Gump! Why did you put that together so quickly?" — "You told me to, Drill Sergeant."
#[derive(Parser)]
#[command(name = "gump", version, about = "Idempotent setup wizard for fresh Fedora, Arch and macOS installs")]
struct Cli {
    /// Read the catalog from this directory instead of the one built into the binary.
    #[arg(long, global = true, value_name = "PATH")]
    catalog: Option<PathBuf>,

    #[command(flatten)]
    flags: RunFlags,

    #[command(subcommand)]
    command: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Launch the interactive wizard (the default).
    Drill,
    /// List the catalog for this OS.
    List {
        /// Only show this category.
        #[arg(long)]
        category: Option<String>,
    },
    /// Show what is installed and the state of each app's settings files.
    Status,
    /// Install apps without the wizard.
    Install {
        /// App ids.
        #[arg(required = true)]
        targets: Vec<String>,
        /// Don't ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
}

#[derive(Args, Clone, Copy, Default)]
struct RunFlags {
    /// Replace existing settings files (the old file is kept as <name>.gump-bak-<time>).
    #[arg(long = "override", global = true)]
    override_settings: bool,
    /// Install apps only; never touch settings files.
    #[arg(long, global = true)]
    agnostic: bool,
    /// Show what would happen without changing anything.
    #[arg(long, global = true)]
    dry_run: bool,
    /// Show the plan for approval and the commands' output.
    #[arg(short, long, global = true)]
    verbose: bool,
}

impl From<RunFlags> for Options {
    fn from(f: RunFlags) -> Options {
        Options { override_settings: f.override_settings, agnostic: f.agnostic, dry_run: f.dry_run, verbose: f.verbose || f.dry_run }
    }
}

fn main() {
    // Exit quietly when piped into `head` and the like, instead of panicking on EPIPE.
    #[cfg(unix)]
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    if let Err(e) = real_main() {
        eprintln!("gump: {e:#}");
        std::process::exit(1);
    }
}

fn real_main() -> Result<()> {
    let cli = Cli::parse();
    let os = platform::detect()?;
    let cat = Catalog::load(cli.catalog.as_deref())?;

    let flags = cli.flags;
    match cli.command.unwrap_or(Cmd::Drill) {
        Cmd::Drill => {
            privilege::ensure_not_root()?;
            tui::run(&cat, os, flags.into())
        }
        Cmd::List { category } => list(&cat, os, category.as_deref()),
        Cmd::Status => status(&cat, os),
        Cmd::Install { targets, yes } => {
            privilege::ensure_not_root()?;
            install(&cat, os, &targets, yes, flags.into())
        }
    }
}

fn list(cat: &Catalog, os: platform::Os, only: Option<&str>) -> Result<()> {
    if let Some(c) = only {
        if !cat.categories.iter().any(|x| x.id == c) {
            bail!("unknown category '{c}'");
        }
    }
    println!("Catalog ({}) for {}\n", cat.source, os.display());
    for c in cat.categories.iter().filter(|c| only.is_none_or(|o| o == c.id)) {
        let apps: Vec<_> = cat.apps.iter().filter(|a| a.category == c.id).collect();
        if apps.is_empty() {
            continue;
        }
        println!("{} [{}]", c.name, c.id);
        for a in apps {
            let avail = if a.available(os) { "" } else { "  (n/a on this OS)" };
            let cfg = if a.has_settings(os) { " +settings" } else { "" };
            println!("  {:<22} {}{cfg}{avail}", a.id, a.description);
        }
        println!();
    }
    Ok(())
}

fn status(cat: &Catalog, os: platform::Os) -> Result<()> {
    let statuses = plan::scan(cat, os);
    println!("{:<22} {:<14} SETTINGS", "APP", "INSTALLED");
    for (a, s) in cat.apps.iter().zip(&statuses) {
        if !s.available {
            continue;
        }
        let inst = if s.installed { "yes" } else { "no" };
        let cfg = match s.settings {
            SettingsBadge::None => "-",
            SettingsBadge::Missing => "missing",
            SettingsBadge::Differs => "differs",
            SettingsBadge::Identical => "up to date",
        };
        println!("{:<22} {:<14} {cfg}", a.id, inst);
    }
    Ok(())
}

fn install(cat: &Catalog, os: platform::Os, targets: &[String], yes: bool, opts: Options) -> Result<()> {
    let plan = plan::build(cat, targets, os, opts)?;
    if opts.verbose {
        for l in plan::describe(&plan) {
            let mark = match l.tone {
                Tone::Header => "▸ ",
                _ => "  ",
            };
            println!("{mark}{}", l.text);
        }
        println!();
    }
    if plan.is_noop() {
        println!("Nothing to do — everything is already in place.");
        return Ok(());
    }
    if opts.verbose && !yes && !opts.dry_run {
        print!("Proceed? [y/N] ");
        std::io::stdout().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim(), "y" | "Y" | "yes") {
            println!("Aborted.");
            return Ok(());
        }
    }
    if plan.needs_sudo() && !opts.dry_run {
        privilege::acquire()?;
    }

    let mut any_failed = false;
    for ev in executor::spawn(plan) {
        match ev {
            Event::Log(l) if opts.verbose => println!("{l}"),
            Event::Log(_) => {}
            Event::Done(s) => {
                println!("Done: {} changed, {} unchanged, {} failed.", s.changed, s.unchanged, s.failed.len());
                for (label, msg) in &s.failed {
                    println!("  ✗ {label}: {msg}");
                }
                if let Some(p) = &s.log_path {
                    println!("Log: {}", settings::tilde(p));
                }
                any_failed = !s.failed.is_empty();
            }
            Event::Started(_) | Event::Finished(..) => {}
        }
    }
    if any_failed {
        std::process::exit(2);
    }
    Ok(())
}
