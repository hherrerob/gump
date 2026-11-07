//! The `gump drill` wizard. It renders inline (a fixed-height area under the prompt, like fzf)
//! instead of taking over the screen; install logs scroll into normal terminal history above it.
//! Flow: Select → Review → Run → Done.

mod art;
mod theme;
mod ui;

use crate::catalog::Catalog;
use crate::executor::{self, Event, Outcome};
use crate::plan::{self, AppStatus, DescLine, Options, Plan};
use crate::platform::Os;
use crate::privilege;
use crate::settings;
use anyhow::Result;
use ratatui::crossterm::event::{self, Event as TermEvent, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::ListState;
use ratatui::{DefaultTerminal, TerminalOptions, Viewport};
use std::collections::BTreeSet;
use std::sync::mpsc::Receiver;
use std::time::Duration;

/// Bounds for the inline wizard area, in terminal rows.
const MIN_HEIGHT: u16 = 10;
const MAX_HEIGHT: u16 = 20;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Select,
    Review,
    Run,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    Categories,
    Apps,
}

#[derive(Clone)]
pub enum UnitState {
    Pending,
    Running,
    Done(Outcome),
}

pub struct RunState {
    rx: Receiver<Event>,
    pub plan: Plan,
    pub units: Vec<UnitState>,
    /// Log lines not yet printed above the viewport.
    pending: Vec<String>,
    pub done: bool,
}

impl RunState {
    pub fn counts(&self) -> (usize, usize, usize) {
        let mut c = (0, 0, 0);
        for u in &self.units {
            match u {
                UnitState::Done(Outcome::Changed) => c.0 += 1,
                UnitState::Done(Outcome::Unchanged) => c.1 += 1,
                UnitState::Done(Outcome::Failed(_)) => c.2 += 1,
                _ => {}
            }
        }
        c
    }
}

pub struct Wizard<'a> {
    pub cat: &'a Catalog,
    pub os: Os,
    pub opts: Options,
    pub status: Vec<AppStatus>,
    pub selected: BTreeSet<usize>,
    pub screen: Screen,
    pub focus: Focus,
    /// Categories that have at least one app, as indices into `cat.categories`.
    pub cats: Vec<usize>,
    pub cat_state: ListState,
    pub app_state: ListState,
    pub filter: String,
    pub filtering: bool,
    pub review: Option<Result<Plan, String>>,
    pub review_lines: Vec<DescLine>,
    pub review_scroll: u16,
    pub run: Option<RunState>,
    /// Summaries of finished runs, printed above the wizard area when it closes.
    finished: Vec<Line<'static>>,
    pub flash: Option<String>,
    quit: bool,
    start_run: bool,
    ctrl_c: bool,
}

impl<'a> Wizard<'a> {
    fn new(cat: &'a Catalog, os: Os, opts: Options, status: Vec<AppStatus>) -> Self {
        let cats: Vec<usize> = (0..cat.categories.len())
            .filter(|&i| cat.apps.iter().any(|a| a.category == cat.categories[i].id))
            .collect();
        Wizard {
            cat,
            os,
            opts,
            status,
            selected: BTreeSet::new(),
            screen: Screen::Select,
            focus: Focus::Apps,
            cats,
            cat_state: ListState::default().with_selected(Some(0)),
            app_state: ListState::default().with_selected(Some(0)),
            filter: String::new(),
            filtering: false,
            review: None,
            review_lines: Vec::new(),
            review_scroll: 0,
            run: None,
            finished: Vec::new(),
            flash: None,
            quit: false,
            start_run: false,
            ctrl_c: false,
        }
    }

    /// App indices shown in the right pane: the current category, or every match while filtering.
    pub fn visible_apps(&self) -> Vec<usize> {
        let f = self.filter.to_lowercase();
        if !f.is_empty() {
            return (0..self.cat.apps.len())
                .filter(|&i| {
                    let a = &self.cat.apps[i];
                    a.id.contains(&f) || a.name.to_lowercase().contains(&f) || a.description.to_lowercase().contains(&f)
                })
                .collect();
        }
        let Some(&ci) = self.cat_state.selected().and_then(|s| self.cats.get(s)) else { return Vec::new() };
        let cid = &self.cat.categories[ci].id;
        (0..self.cat.apps.len()).filter(|&i| &self.cat.apps[i].category == cid).collect()
    }

    pub fn current_app(&self) -> Option<usize> {
        self.app_state.selected().and_then(|s| self.visible_apps().get(s).copied())
    }

    fn toggle(&mut self, i: usize) {
        if !self.status[i].available {
            self.flash = Some(format!("{} is not available on {}", self.cat.apps[i].name, self.os.display()));
        } else if !self.selected.remove(&i) {
            self.selected.insert(i);
        }
    }

    fn build_review(&mut self) {
        let ids: Vec<String> = self.selected.iter().map(|&i| self.cat.apps[i].id.clone()).collect();
        match plan::build(self.cat, &ids, self.os, self.opts) {
            Ok(p) => {
                self.review_lines = plan::describe(&p);
                self.review = Some(Ok(p));
            }
            Err(e) => {
                self.review_lines = Vec::new();
                self.review = Some(Err(format!("{e:#}")));
            }
        }
        self.review_scroll = 0;
    }

    fn on_key(&mut self, k: KeyEvent) {
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            let running = self.run.as_ref().is_some_and(|r| !r.done);
            if running && !self.ctrl_c {
                self.ctrl_c = true;
                self.flash = Some("Install in progress. Press Ctrl-C again to quit anyway.".into());
            } else {
                self.quit = true;
            }
            return;
        }
        self.flash = None;
        match self.screen {
            Screen::Select => self.on_select_key(k),
            Screen::Review => match k.code {
                KeyCode::Esc | KeyCode::Backspace => self.screen = Screen::Select,
                KeyCode::Char('q') => self.quit = true,
                KeyCode::Up | KeyCode::Char('k') => self.review_scroll = self.review_scroll.saturating_sub(1),
                KeyCode::Down | KeyCode::Char('j') => self.review_scroll = self.review_scroll.saturating_add(1),
                KeyCode::Char('o') => {
                    self.opts.override_settings = !self.opts.override_settings;
                    self.build_review();
                }
                KeyCode::Char('g') => {
                    self.opts.agnostic = !self.opts.agnostic;
                    self.build_review();
                }
                KeyCode::Char('d') => {
                    self.opts.dry_run = !self.opts.dry_run;
                    self.build_review();
                }
                KeyCode::Enter => match &self.review {
                    Some(Ok(p)) if p.is_noop() => {
                        self.flash = Some("Nothing to do — everything is already in place.".into())
                    }
                    Some(Ok(_)) => self.start_run = true,
                    _ => {}
                },
                _ => {}
            },
            Screen::Run => {
                let done = self.run.as_ref().is_some_and(|r| r.done);
                match k.code {
                    KeyCode::Char('q') | KeyCode::Esc if done => self.quit = true,
                    KeyCode::Enter if done => {
                        self.status = plan::scan(self.cat, self.os);
                        self.selected.clear();
                        self.run = None;
                        self.ctrl_c = false;
                        self.screen = Screen::Select;
                    }
                    _ => {}
                }
            }
        }
    }

    fn on_select_key(&mut self, k: KeyEvent) {

        if self.filtering {
            match k.code {
                KeyCode::Esc => {
                    self.filtering = false;
                    self.filter.clear();
                }
                KeyCode::Enter | KeyCode::Down | KeyCode::Up => {
                    self.filtering = false;
                    self.focus = Focus::Apps;
                }
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) => self.filter.push(c),
                _ => {}
            }
            self.app_state.select(Some(0));
            return;
        }

        let n_apps = self.visible_apps().len();
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Esc if !self.filter.is_empty() => {
                self.filter.clear();
                self.app_state.select(Some(0));
            }
            KeyCode::Esc => self.quit = true,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::Apps { Focus::Categories } else { Focus::Apps }
            }
            KeyCode::Left | KeyCode::Char('h') => self.focus = Focus::Categories,
            KeyCode::Right | KeyCode::Char('l') => self.focus = Focus::Apps,
            KeyCode::Up | KeyCode::Char('k') => match self.focus {
                Focus::Categories => {
                    self.cat_state.select_previous();
                    self.app_state.select(Some(0));
                }
                Focus::Apps => self.app_state.select_previous(),
            },
            KeyCode::Down | KeyCode::Char('j') => match self.focus {
                Focus::Categories => {
                    let next = self.cat_state.selected().map_or(0, |s| (s + 1).min(self.cats.len().saturating_sub(1)));
                    self.cat_state.select(Some(next));
                    self.app_state.select(Some(0));
                }
                Focus::Apps => {
                    let next = self.app_state.selected().map_or(0, |s| (s + 1).min(n_apps.saturating_sub(1)));
                    self.app_state.select(Some(next));
                }
            },
            KeyCode::Char(' ') if self.focus == Focus::Apps => {
                if let Some(i) = self.current_app() {
                    self.toggle(i);
                }
            }
            KeyCode::Char('c') => self.selected.clear(),
            KeyCode::Char('/') => {
                self.filtering = true;
                self.focus = Focus::Apps;
            }
            KeyCode::Enter => {
                if self.selected.is_empty() {
                    self.flash = Some("Select at least one app (Space)".into());
                } else {
                    self.build_review();
                    if self.opts.verbose {
                        self.screen = Screen::Review;
                    } else {
                        // No review screen: go straight to the run (or say why not).
                        match &self.review {
                            Some(Ok(p)) if p.is_noop() => {
                                self.flash = Some("Nothing to do — everything is already in place.".into())
                            }
                            Some(Ok(_)) => self.start_run = true,
                            Some(Err(e)) => self.flash = Some(e.clone()),
                            None => {}
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn pump(&mut self) {
        let Some(run) = &mut self.run else { return };
        while let Ok(ev) = run.rx.try_recv() {
            match ev {
                Event::Started(i) => run.units[i] = UnitState::Running,
                Event::Log(l) if run.plan.opts.verbose => run.pending.push(l),
                Event::Log(_) => {}
                Event::Finished(i, o) => run.units[i] = UnitState::Done(o),
                Event::Done(summary) => {
                    run.done = true;
                    let (changed, unchanged, failed) = (summary.changed, summary.unchanged, summary.failed.len());
                    let dry = if run.plan.opts.dry_run { "  dry run" } else { "" };
                    let color = if failed > 0 { theme::RED } else { theme::TEAL };
                    let mut lines = vec![
                        Line::raw(""),
                        Line::from(vec![
                            Span::styled(" gump ", theme::pill(theme::PRIMARY)),
                            Span::styled(format!(" {changed} changed "), theme::pill(color)),
                            Span::styled(format!("  {unchanged} unchanged · {failed} failed"), theme::fg(theme::TEXT)),
                            Span::styled(dry, theme::bold(theme::ORANGE)),
                        ]),
                    ];
                    for (label, msg) in summary.failed {
                        lines.push(Line::from(vec![
                            Span::styled("  ✗ ", theme::bold(theme::RED)),
                            Span::styled(format!("{label}: "), theme::bold(theme::TEXT)),
                            Span::styled(msg, theme::fg(theme::RED)),
                        ]));
                    }
                    if let Some(p) = &summary.log_path {
                        lines.push(Line::styled(format!("  log  {}", settings::tilde(p)), theme::fg(theme::MUTED)));
                    }
                    let seed = (changed * 7 + unchanged * 3) + self.finished.len();
                    lines.push(Line::styled(
                        format!("  “{}”", art::sign_off(failed > 0, seed)),
                        theme::fg(theme::SOFT).italic(),
                    ));
                    self.finished.extend(lines);
                }
            }
        }
    }
}

/// Rows needed to show the longest list without scrolling: list + 2 borders + header, details, footer.
fn wanted_height(cat: &Catalog) -> u16 {
    let longest = cat.categories.iter().map(|c| cat.apps.iter().filter(|a| a.category == c.id).count());
    let n = longest.chain([cat.categories.len()]).max().unwrap_or(0) as u16;
    (n + 5).clamp(MIN_HEIGHT, MAX_HEIGHT)
}

fn init_terminal(height: u16) -> DefaultTerminal {
    let rows = ratatui::crossterm::terminal::size().map(|(_, h)| h).unwrap_or(height);
    ratatui::init_with_options(TerminalOptions {
        viewport: Viewport::Inline(height.min(rows.saturating_sub(1)).max(6)),
    })
}

/// Erase the inline area and drop raw mode, leaving the cursor where the wizard started.
fn close_terminal(terminal: &mut DefaultTerminal) {
    let _ = terminal.clear();
    ratatui::restore();
}

pub fn run(cat: &Catalog, os: Os, opts: Options) -> Result<()> {
    let status = plan::scan(cat, os);
    let available = status.iter().filter(|s| s.available).count();
    let installed = status.iter().filter(|s| s.available && s.installed).count();
    let source = match &cat.source {
        crate::catalog::Source::Embedded => "embedded".to_string(),
        crate::catalog::Source::Disk(p) => crate::settings::tilde(p),
    };

    let mut wiz = Wizard::new(cat, os, opts, status);
    let height = wanted_height(cat);
    let mut terminal = init_terminal(height);
    let width = terminal.size().map(|s| s.width).unwrap_or(80);
    let pill = format!(" {} ", os.display());
    let mut detail = format!("  {available} apps · {installed} installed · catalog {source}");
    // Keep the info line on one row; the banner indents it by up to 39 columns.
    let room = (width as usize).saturating_sub(pill.chars().count() + 40);
    if detail.chars().count() > room {
        detail = detail.chars().take(room.saturating_sub(1)).collect::<String>() + "…";
    }
    let info = vec![Span::styled(pill, theme::pill(theme::CYAN)), Span::styled(detail, theme::fg(theme::MUTED))];
    let mut banner = art::banner(width, info);
    banner.push(Line::raw(""));
    ui::print_above(&mut terminal, banner)?;
    let result = (|| -> Result<()> {
        loop {
            wiz.pump();
            if let Some(run) = &mut wiz.run {
                ui::flush_log(&mut terminal, &mut run.pending)?;
            }
            terminal.draw(|f| ui::draw(f, &mut wiz))?;
            if event::poll(Duration::from_millis(50))? {
                match event::read()? {
                    TermEvent::Key(k) if k.kind == KeyEventKind::Press => wiz.on_key(k),
                    TermEvent::Resize(..) => terminal.autoresize()?,
                    _ => {}
                }
            }
            if wiz.start_run {
                wiz.start_run = false;
                let Some(Ok(plan)) = wiz.review.take() else { continue };
                if plan.needs_sudo() && !plan.opts.dry_run {
                    // Step out of the wizard so sudo can prompt normally, then start a fresh area.
                    close_terminal(&mut terminal);
                    let auth = privilege::acquire();
                    terminal = init_terminal(height);
                    if let Err(e) = auth {
                        wiz.flash = Some(format!("{e:#}"));
                        wiz.review = Some(Ok(plan));
                        continue;
                    }
                }
                let units = vec![UnitState::Pending; plan.units.len()];
                let rx = executor::spawn(plan.clone());
                wiz.run = Some(RunState { rx, plan, units, pending: Vec::new(), done: false });
                wiz.screen = Screen::Run;
            }
            if wiz.quit {
                return Ok(());
            }
        }
    })();
    if wiz.finished.is_empty() && result.is_ok() {
        wiz.finished.push(Line::styled("  At ease. Nothing was installed.", theme::fg(theme::MUTED).italic()));
    }
    let _ = ui::print_above(&mut terminal, std::mem::take(&mut wiz.finished));
    close_terminal(&mut terminal);
    result
}
