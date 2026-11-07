//! Runs a plan on a background thread and streams progress back over a channel.

use crate::backend::Cmd;
use crate::plan::{FileAction, Plan, Unit};
use crate::privilege;
use crate::settings;
use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::ffi::OsString;
use std::fs::File;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;

#[derive(Clone, Debug)]
pub enum Outcome {
    Changed,
    Unchanged,
    Failed(String),
}

#[derive(Debug)]
pub enum Event {
    Started(usize),
    Log(String),
    Finished(usize, Outcome),
    Done(Summary),
}

/// The result of a whole run, computed once here for both the wizard and `gump install`.
#[derive(Debug, Default)]
pub struct Summary {
    pub changed: usize,
    pub unchanged: usize,
    /// `(unit label, error message)` for every failed unit.
    pub failed: Vec<(String, String)>,
    pub log_path: Option<PathBuf>,
}

pub fn spawn(plan: Plan) -> Receiver<Event> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || run(plan, tx));
    rx
}

struct Logger {
    tx: Sender<Event>,
    file: Option<File>,
}

impl Logger {
    fn line(&mut self, s: impl Into<String>) {
        let s = s.into();
        if let Some(f) = &mut self.file {
            let _ = writeln!(f, "{s}");
        }
        let _ = self.tx.send(Event::Log(s));
    }
}

fn run(plan: Plan, tx: Sender<Event>) {
    let log_path = settings::state_dir().ok().map(|d| d.join("last-run.log"));
    let file = log_path.as_ref().and_then(|p| {
        std::fs::create_dir_all(p.parent()?).ok()?;
        File::create(p).ok()
    });
    let mut log = Logger { tx: tx.clone(), file };
    let path = child_path(plan.opts.dry_run);
    let mut failed: HashSet<String> = HashSet::new();
    let mut summary = Summary { log_path, ..Summary::default() };

    for (i, unit) in plan.units.iter().enumerate() {
        let _ = tx.send(Event::Started(i));
        log.line(format!("── {} ──", unit.label));
        let outcome = match unit.depends.iter().find(|d| failed.contains(*d)) {
            Some(d) => Outcome::Failed(format!("dependency '{d}' failed")),
            None => match run_unit(unit, &plan, &path, &mut log) {
                Ok(o) => o,
                Err(e) => Outcome::Failed(format!("{e:#}")),
            },
        };
        match &outcome {
            Outcome::Changed => summary.changed += 1,
            Outcome::Unchanged => summary.unchanged += 1,
            Outcome::Failed(msg) => {
                log.line(format!("✗ {msg}"));
                failed.insert(unit.id.clone());
                summary.failed.push((unit.label.clone(), msg.clone()));
            }
        }
        let _ = tx.send(Event::Finished(i, outcome));
    }
    let _ = tx.send(Event::Done(summary));
}

fn run_unit(unit: &Unit, plan: &Plan, path: &OsString, log: &mut Logger) -> Result<Outcome> {
    let dry = plan.opts.dry_run;
    if unit.is_noop() {
        for f in &unit.files {
            log.line(format!("kept {}", settings::tilde(&f.target)));
        }
        log.line("nothing to do");
        return Ok(Outcome::Unchanged);
    }
    for cmd in unit.cmds.iter().chain(&unit.extras) {
        let shown = cmd.display();
        let mut lines = shown.lines();
        log.line(format!("{} {}", if dry { "[dry-run] would run:" } else { "$" }, lines.next().unwrap_or_default()));
        for l in lines {
            log.line(format!("    {l}"));
        }
        if !dry {
            run_cmd(cmd, path, log)?;
        }
    }
    for f in &unit.files {
        let p = settings::tilde(&f.target);
        match f.action {
            FileAction::SkipIdentical => log.line(format!("settings up to date: {p}")),
            FileAction::SkipExisting => log.line(format!("kept existing settings: {p}")),
            FileAction::Copy | FileAction::Override if dry => log.line(format!("[dry-run] would write {p}")),
            FileAction::Copy | FileAction::Override => {
                match settings::write_file(&f.target, &f.content, f.action == FileAction::Override)? {
                    Some(bak) => log.line(format!("wrote {p} (backup: {})", settings::tilde(&bak))),
                    None => log.line(format!("wrote {p}")),
                }
            }
        }
    }
    Ok(Outcome::Changed)
}

/// PATH for children: sudo shim first, and Homebrew's locations on macOS (so a freshly
/// bootstrapped brew is found without a new shell).
fn child_path(dry_run: bool) -> OsString {
    let mut parts: Vec<PathBuf> = Vec::new();
    if !dry_run {
        parts.extend(privilege::shim_dir());
    }
    if let Some(p) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&p));
    }
    if cfg!(target_os = "macos") {
        for p in ["/opt/homebrew/bin", "/usr/local/bin"] {
            let p = PathBuf::from(p);
            if !parts.contains(&p) {
                parts.push(p);
            }
        }
    }
    std::env::join_paths(parts).unwrap_or_default()
}

fn run_cmd(cmd: &Cmd, path: &OsString, log: &mut Logger) -> Result<()> {
    let mut c = if cmd.program == "sudo" {
        let mut c = Command::new(privilege::real_sudo());
        c.arg("-n");
        c
    } else {
        Command::new(&cmd.program)
    };
    c.args(&cmd.args)
        .env("PATH", path)
        .envs(cmd.env.iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = c.spawn().with_context(|| format!("could not start '{}'", cmd.program))?;

    let (ltx, lrx) = mpsc::channel::<String>();
    let readers: Vec<_> = [
        child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
    ]
    .into_iter()
    .flatten()
    .map(|r| {
        let ltx = ltx.clone();
        thread::spawn(move || stream_lines(r, ltx))
    })
    .collect();
    drop(ltx);
    for line in lrx {
        log.line(format!("  {line}"));
    }
    for r in readers {
        let _ = r.join();
    }
    let status = child.wait()?;
    if !status.success() {
        bail!("`{}` exited with {}", first_line(&cmd.display()), status);
    }
    Ok(())
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

/// Split output on `\n` and `\r` (so progress bars become separate lines) and strip ANSI codes.
fn stream_lines(mut r: Box<dyn Read + Send>, tx: Sender<String>) {
    let mut buf = [0u8; 4096];
    let mut cur: Vec<u8> = Vec::new();
    let emit = |cur: &mut Vec<u8>| {
        let s = strip_ansi(&String::from_utf8_lossy(cur));
        let s = s.trim_end();
        if !s.is_empty() {
            let _ = tx.send(s.to_string());
        }
        cur.clear();
    };
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(n) => {
                for &b in &buf[..n] {
                    if b == b'\n' || b == b'\r' {
                        emit(&mut cur);
                    } else {
                        cur.push(b);
                    }
                }
            }
        }
    }
    emit(&mut cur);
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // CSI: ESC [ params final-byte(@..~). Other escapes: drop the next char.
            if let Some('[') = chars.next() {
                for n in chars.by_ref() {
                    if ('@'..='~').contains(&n) {
                        break;
                    }
                }
            }
        } else if c != '\u{7}' {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ansi() {
        assert_eq!(strip_ansi("\u{1b}[1;32mok\u{1b}[0m done"), "ok done");
    }
}
