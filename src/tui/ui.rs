use super::theme::{self as t, Rgb};
use super::{Focus, Screen, UnitState, Wizard};
use crate::executor::Outcome;
use crate::plan::{SettingsBadge, Tone, UnitKind};
use anyhow::Result;
use ratatui::buffer::Buffer;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, HighlightSpacing, List, ListItem, Paragraph, Widget, Wrap};
use ratatui::{DefaultTerminal, Frame};

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

pub fn draw(f: &mut Frame, w: &mut Wizard) {
    let [body, footer] = Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(f.area());
    match w.screen {
        Screen::Select => select(f, w, body),
        Screen::Review => review(f, w, body),
        Screen::Run => run(f, w, body),
    }
    let done = w.run.as_ref().is_some_and(|r| r.done);
    let keys: &[(&str, &str)] = match w.screen {
        Screen::Select if w.filtering => &[("type", "filter"), ("enter", "done"), ("esc", "clear")],
        Screen::Select => &[
            ("↑↓", "move"),
            ("←→", "pane"),
            ("space", "toggle"),
            ("/", "search"),
            ("c", "clear"),
            ("enter", "review"),
            ("q", "quit"),
        ],
        Screen::Review => &[
            ("enter", "run"),
            ("o", "override"),
            ("g", "agnostic"),
            ("d", "dry-run"),
            ("↑↓", "scroll"),
            ("esc", "back"),
            ("q", "quit"),
        ],
        Screen::Run if done => &[("enter", "pick more"), ("q", "quit")],
        Screen::Run => &[("ctrl-c ×2", "abort")],
    };
    let line = match &w.flash {
        Some(m) => {
            Line::from(vec![Span::styled(" ! ", t::pill(t::ORANGE)), Span::styled(format!(" {m}"), t::bold(t::ORANGE))])
        }
        None => hints(keys),
    };
    f.render_widget(Paragraph::new(line), footer);
}

fn hints(keys: &[(&str, &str)]) -> Line<'static> {
    let mut spans = Vec::new();
    for (i, (k, d)) in keys.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", t::fg(t::LINE)));
        }
        spans.push(Span::styled(k.to_string(), t::bold(t::PRIMARY)));
        spans.push(Span::styled(format!(" {d}"), t::fg(t::MUTED)));
    }
    Line::from(spans)
}

fn block(title: String, focused: bool) -> Block<'static> {
    let (border, title_style) =
        if focused { (t::fg(t::PRIMARY), t::bold(t::PRIMARY)) } else { (t::fg(t::LINE), t::bold(t::SOFT)) };
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(border)
        .title(Span::styled(format!(" {title} "), title_style))
}

fn flag_spans(w: &Wizard) -> Vec<Span<'static>> {
    let flag = |on: bool, name: &'static str| {
        if on {
            vec![Span::raw(" "), Span::styled(format!(" {name} "), t::pill(t::YELLOW))]
        } else {
            vec![Span::raw(" "), Span::styled(format!(" {name} "), t::fg(t::LINE))]
        }
    };
    [flag(w.opts.override_settings, "override"), flag(w.opts.agnostic, "agnostic"), flag(w.opts.dry_run, "dry-run")]
        .concat()
}

/// `▌GUMP▐ title · detail      flags`
fn header(f: &mut Frame, area: Rect, title: &str, detail: String, w: &Wizard) {
    let left = Line::from(vec![
        Span::styled(" gump ", t::pill(t::PRIMARY)),
        Span::styled(format!(" {title} "), t::pill(t::SECONDARY)),
        Span::styled(format!("  {detail}"), t::fg(t::TEXT)),
    ]);
    let right = Line::from(flag_spans(w)).right_aligned();
    f.render_widget(Paragraph::new(left), area);
    f.render_widget(Paragraph::new(right), area);
}

fn select(f: &mut Frame, w: &mut Wizard, area: Rect) {
    let [top, main, detail_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(3), Constraint::Length(1)]).areas(area);
    let n = w.selected.len();
    let detail = if n == 0 { "pick your gear".to_string() } else { format!("{n} selected") };
    header(f, top, "drill", detail, w);

    let [left, right] = Layout::horizontal([Constraint::Length(24), Constraint::Min(30)]).areas(main);

    let items: Vec<ListItem> = w
        .cats
        .iter()
        .map(|&ci| {
            let c = &w.cat.categories[ci];
            let idx: Vec<usize> = (0..w.cat.apps.len()).filter(|&i| w.cat.apps[i].category == c.id).collect();
            let sel = idx.iter().filter(|i| w.selected.contains(i)).count();
            let count = if sel > 0 {
                Span::styled(format!(" {sel}/{}", idx.len()), t::bold(t::TEAL))
            } else {
                Span::styled(format!(" {}", idx.len()), t::fg(t::MUTED))
            };
            ListItem::new(Line::from(vec![Span::styled(c.name.clone(), t::fg(t::TEXT)), count]))
        })
        .collect();
    let cats_focused = w.focus == Focus::Categories && w.filter.is_empty();
    let cat_list = List::new(items)
        .block(block("Categories".into(), cats_focused))
        .highlight_style(t::highlight())
        .highlight_symbol("❯ ")
        .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(cat_list, left, &mut w.cat_state);

    let visible = w.visible_apps();
    let items: Vec<ListItem> = visible
        .iter()
        .map(|&i| {
            let a = &w.cat.apps[i];
            let s = w.status[i];
            let (mark, name_style) = if !s.available {
                (Span::styled("·  ", t::fg(t::LINE)), t::fg(t::LINE))
            } else if w.selected.contains(&i) {
                (Span::styled("◉  ", t::bold(t::PRIMARY)), t::bold(t::TEXT))
            } else {
                (Span::styled("○  ", t::fg(t::MUTED)), t::fg(t::TEXT))
            };
            let mut spans = vec![mark, Span::styled(format!("{:<22}", a.name), name_style)];
            if !s.available {
                spans.push(Span::styled("not on this OS", t::fg(t::LINE).italic()));
            } else {
                if s.installed {
                    spans.push(Span::styled("✓ installed  ", t::fg(t::TEAL)));
                }
                let badge: Option<(&str, Rgb)> = match s.settings {
                    SettingsBadge::None => None,
                    SettingsBadge::Missing => Some(("◆ settings new", t::ORANGE)),
                    SettingsBadge::Differs => Some(("◆ settings yours", t::PINK)),
                    SettingsBadge::Identical => Some(("◆ settings synced", t::CYAN)),
                };
                if let Some((txt, col)) = badge {
                    spans.push(Span::styled(txt, t::fg(col)));
                }
            }
            ListItem::new(Line::from(spans))
        })
        .collect();
    let title = if w.filtering || !w.filter.is_empty() {
        format!("⌕ {}{}", w.filter, if w.filtering { "▏" } else { "" })
    } else {
        w.cat_state
            .selected()
            .and_then(|s| w.cats.get(s))
            .map(|&ci| w.cat.categories[ci].name.clone())
            .unwrap_or_default()
    };
    let app_list = List::new(items)
        .block(block(title, w.focus == Focus::Apps))
        .highlight_style(t::highlight())
        .highlight_symbol("❯ ")
        .highlight_spacing(HighlightSpacing::Always);
    f.render_stateful_widget(app_list, right, &mut w.app_state);

    // One-line details for the highlighted app.
    let detail = match w.current_app() {
        Some(i) => {
            let a = &w.cat.apps[i];
            let mut spans =
                vec![Span::styled(" ❯ ", t::bold(t::PRIMARY)), Span::styled(a.description.clone(), t::fg(t::SOFT))];
            if !a.depends.is_empty() {
                spans.push(Span::styled(format!("  needs {}", a.depends.join(", ")), t::fg(t::MUTED)));
            }
            let targets: Vec<&str> = a.settings.iter().filter_map(|s| s.target_for(w.os)).collect();
            if !targets.is_empty() {
                spans.push(Span::styled(format!("  → {}", targets.join(", ")), t::fg(t::MUTED)));
            }
            Line::from(spans)
        }
        None => Line::from(Span::styled(" ❯ nothing matches", t::fg(t::MUTED))),
    };
    f.render_widget(Paragraph::new(detail), detail_area);
}

/// Plan lines look like `  run       sudo dnf …`: color the verb column separately.
fn review_line(tone: Tone, text: &str) -> Line<'static> {
    let split = |verb: Rgb, rest: Style| {
        let cut = text.char_indices().nth(12).map_or(text.len(), |(i, _)| i);
        Line::from(vec![
            Span::styled(text[..cut].to_string(), t::fg(verb)),
            Span::styled(text[cut..].to_string(), rest),
        ])
    };
    match tone {
        Tone::Header => {
            Line::from(vec![Span::styled("▌", t::fg(t::PRIMARY)), Span::styled(text.to_string(), t::bold(t::PRIMARY))])
        }
        Tone::Action => split(t::CYAN, t::fg(t::TEXT)),
        Tone::Warn => split(t::ORANGE, t::fg(t::ORANGE)),
        Tone::Skip => split(t::LINE, t::fg(t::MUTED)),
    }
}

fn review(f: &mut Frame, w: &mut Wizard, area: Rect) {
    let [top, main] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    header(f, top, "review", format!("{} app(s) — this is the plan", w.selected.len()), w);
    let lines: Vec<Line> = match &w.review {
        Some(Err(e)) => vec![Line::from(Span::styled(format!("Cannot build plan: {e}"), t::bold(t::RED)))],
        _ => w.review_lines.iter().map(|l| review_line(l.tone, &l.text)).collect(),
    };
    let max = (lines.len() as u16).saturating_sub(main.height.saturating_sub(2));
    w.review_scroll = w.review_scroll.min(max);
    let title =
        if max > 0 { format!("What will happen  {}/{}", w.review_scroll, max) } else { "What will happen".into() };
    f.render_widget(Paragraph::new(lines).block(block(title, true)).scroll((w.review_scroll, 0)), main);
}

fn unit_marker(s: &UnitState, frame: usize) -> Span<'static> {
    const SPIN: [&str; 4] = ["◐", "◓", "◑", "◒"];
    match s {
        UnitState::Pending => Span::styled("·", t::fg(t::LINE)),
        UnitState::Running => Span::styled(SPIN[frame % 4], t::bold(t::PRIMARY)),
        UnitState::Done(Outcome::Changed) => Span::styled("✓", t::bold(t::TEAL)),
        UnitState::Done(Outcome::Unchanged) => Span::styled("=", t::fg(t::MUTED)),
        UnitState::Done(Outcome::Failed(_)) => Span::styled("✗", t::bold(t::RED)),
    }
}

/// Progress as a sunset-gradient track with Forrest running along it.
fn progress(f: &mut Frame, area: Rect, ratio: f64, done: bool, failed: bool, label: String, frame: usize) {
    const RUNNER: [&str; 2] = ["┌(^_^)┘", "└(^_^)┐"];
    let label_w = label.chars().count() as u16 + 1;
    let track_w = area.width.saturating_sub(label_w + 8).max(4) as usize;
    let filled = ((track_w as f64) * ratio).round() as usize;
    let mut spans = Vec::new();
    for i in 0..track_w {
        let col = t::gradient(&t::SUNSET, i as f32 / track_w as f32);
        if i < filled {
            spans.push(Span::styled("━", t::fg(col)));
        } else {
            spans.push(Span::styled("─", t::fg(t::LINE)));
        }
    }
    let runner = if failed {
        Span::styled(" (o_O)? ", t::bold(t::RED))
    } else if done {
        Span::styled(" \\(^o^)/ ", t::bold(t::TEAL))
    } else {
        Span::styled(format!(" {} ", RUNNER[frame / 3 % 2]), t::bold(t::YELLOW))
    };
    spans.insert(filled.min(track_w), runner);
    let color = if failed {
        t::RED
    } else if done {
        t::TEAL
    } else {
        t::PRIMARY
    };
    spans.push(Span::styled(format!(" {label}"), t::bold(color)));
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn run(f: &mut Frame, w: &mut Wizard, area: Rect) {
    let Some(r) = &w.run else { return };
    let frame = (now_ms() / 150) as usize;
    let finished = r.units.iter().filter(|u| matches!(u, UnitState::Done(_))).count();
    let total = r.units.len().max(1);
    let [top, bar, _, main] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1), Constraint::Length(1), Constraint::Min(1)])
            .areas(area);

    let (changed, unchanged, failed) = r.counts();
    let dry = if r.plan.opts.dry_run { " (dry run)" } else { "" };
    let (title, detail) = if r.done {
        ("done", format!("{changed} changed · {unchanged} unchanged · {failed} failed{dry}"))
    } else {
        ("run, forrest, run", format!("{finished}/{total}{dry}"))
    };
    header(f, top, title, detail, w);
    let label = if r.done { "100%".to_string() } else { format!("{}%", finished * 100 / total) };
    progress(f, bar, finished as f64 / total as f64, r.done, failed > 0, label, frame);

    // Units flow into as many columns as fit; page to keep the running one in view.
    let items: Vec<Line> = r
        .plan
        .units
        .iter()
        .zip(&r.units)
        .map(|(u, s)| {
            let tag = if matches!(u.kind, UnitKind::Prereq) { " prereq" } else { "" };
            let name = match s {
                UnitState::Running => t::bold(t::TEXT),
                UnitState::Pending => t::fg(t::MUTED),
                _ => t::fg(t::SOFT),
            };
            Line::from(vec![
                Span::raw(" "),
                unit_marker(s, frame),
                Span::raw(" "),
                Span::styled(u.label.clone(), name),
                Span::styled(tag, t::fg(t::LINE).italic()),
            ])
        })
        .collect();
    let rows = main.height.max(1) as usize;
    let col_w = 28u16;
    let cols = (main.width / col_w).max(1) as usize;
    let per_page = rows * cols;
    let running = r.units.iter().position(|u| matches!(u, UnitState::Running)).unwrap_or(finished);
    let page_start = if items.len() <= per_page { 0 } else { (running / per_page) * per_page };
    for (k, line) in items.into_iter().skip(page_start).take(per_page).enumerate() {
        let (c, row) = (k / rows, k % rows);
        let rect = Rect::new(main.x + c as u16 * col_w, main.y + row as u16, col_w.min(main.width), 1);
        f.render_widget(Paragraph::new(line), rect);
    }
}

/// Style one executor log line for the scrollback.
fn log_line(l: &str) -> Line<'static> {
    if let Some(name) = l.strip_prefix("── ").and_then(|s| s.strip_suffix(" ──")) {
        return Line::from(vec![
            Span::styled(" ❯ ", t::pill(t::PRIMARY)),
            Span::styled(format!(" {name}"), t::bold(t::PRIMARY)),
        ]);
    }
    if let Some(rest) = l.strip_prefix("$ ") {
        return Line::from(vec![
            Span::styled("$ ", t::bold(t::PRIMARY)),
            Span::styled(rest.to_string(), t::fg(t::SOFT)),
        ]);
    }
    if let Some(rest) = l.strip_prefix("[dry-run] ") {
        return Line::from(vec![
            Span::styled("dry-run ", t::bold(t::ORANGE)),
            Span::styled(rest.to_string(), t::fg(t::SOFT)),
        ]);
    }
    let style = if l.starts_with('✗') {
        t::bold(t::RED)
    } else if l.starts_with("wrote") {
        t::fg(t::TEAL)
    } else if l.starts_with("kept") || l.starts_with("settings") || l == "nothing to do" {
        t::fg(t::MUTED).add_modifier(Modifier::ITALIC)
    } else {
        Style::new().fg(t::c((190, 178, 214)))
    };
    Line::from(Span::styled(l.to_string(), style))
}

/// Print lines above the inline area, where they become ordinary scrollback.
pub fn print_above(terminal: &mut DefaultTerminal, lines: Vec<Line<'static>>) -> Result<()> {
    let width = terminal.size()?.width.max(1) as usize;
    // Pre-wrap so each row is exactly one physical line, then insert in small batches.
    let mut rows: Vec<Line> = Vec::new();
    for line in lines {
        let mut cur: Vec<Span> = Vec::new();
        let mut used = 0;
        for span in line.spans {
            let mut buf = String::new();
            for ch in span.content.chars() {
                if used == width {
                    if !buf.is_empty() {
                        cur.push(Span::styled(std::mem::take(&mut buf), span.style));
                    }
                    rows.push(Line::from(std::mem::take(&mut cur)));
                    used = 0;
                }
                buf.push(ch);
                used += 1;
            }
            if !buf.is_empty() {
                cur.push(Span::styled(buf, span.style));
            }
        }
        rows.push(Line::from(cur));
    }
    for batch in rows.chunks(8) {
        let batch = batch.to_vec();
        terminal.insert_before(batch.len() as u16, |buf: &mut Buffer| {
            Paragraph::new(batch).wrap(Wrap { trim: false }).render(buf.area, buf);
        })?;
    }
    Ok(())
}

pub fn flush_log(terminal: &mut DefaultTerminal, pending: &mut Vec<String>) -> Result<()> {
    if pending.is_empty() {
        return Ok(());
    }
    let lines = pending.drain(..).map(|l| log_line(&l)).collect();
    print_above(terminal, lines)
}
