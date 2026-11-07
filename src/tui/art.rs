//! The banner printed above the wizard: the drill sergeant's campaign hat next to the GUMP logo.
//! The art is pixel art drawn with half blocks (two pixels per terminal cell).

use super::theme::{self, Rgb};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

const W: usize = 36;
const H: usize = 18;

const LOGO: [&str; 6] = [
    " ██████╗ ██╗   ██╗███╗   ███╗██████╗ ",
    "██╔════╝ ██║   ██║████╗ ████║██╔══██╗",
    "██║  ███╗██║   ██║██╔████╔██║██████╔╝",
    "██║   ██║██║   ██║██║╚██╔╝██║██╔═══╝ ",
    "╚██████╔╝╚██████╔╝██║ ╚═╝ ██║██║     ",
    " ╚═════╝  ╚═════╝ ╚═╝     ╚═╝╚═╝     ",
];

// The drill sergeant's campaign hat (canvas H is sized so the hat touches its top and bottom rows), seen from the front and slightly above: a wide flat brim,
// a tall crown pinched into a "Montana peak", a leather band and a brass badge.
const CX: f32 = 18.0; // horizontal center
const BRIM: (f32, f32, f32) = (12.5, 17.5, 4.5); // center y, x radius, y radius
const CROWN_BASE: (f32, f32, f32) = (11.0, 8.5, 2.5); // center y, half width, curve depth
const PEAK_Y: f32 = 0.5; // tip of the peak
const SHOULDER: (f32, f32) = (3.5, 7.5); // y and half width where the peak meets the sides

fn in_ellipse(x: f32, y: f32, cy: f32, rx: f32, ry: f32) -> bool {
    let (dx, dy) = ((x - CX) / rx, (y - cy) / ry);
    dx * dx + dy * dy <= 1.0
}

fn pixels() -> [[Option<Rgb>; W]; H] {
    let mut px = [[None; W]; H];
    let dim = |c: Rgb, t: f32| theme::gradient(&[c, theme::INK], t);
    let (by, brx, bry) = BRIM;
    let (cy, chw, depth) = CROWN_BASE;

    for (y, row) in px.iter_mut().enumerate() {
        for (x, p) in row.iter_mut().enumerate() {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            let dx = fx - CX;

            // Crown: sides taper from the base up to the shoulders, then rise to the peak.
            let t = ((cy - fy) / (cy - SHOULDER.0)).clamp(0.0, 1.0);
            let half = chw + (SHOULDER.1 - chw) * t;
            let top = PEAK_Y + dx.abs() * (SHOULDER.0 - PEAK_Y) / SHOULDER.1;
            let curve = depth * (1.0 - (dx / chw).powi(2)).max(0.0).sqrt();
            let bottom = cy + curve;
            if dx.abs() <= half && fy >= top && fy <= bottom {
                let band = fy > bottom - 2.5;
                let crease = dx.abs() < 0.6 && fy < SHOULDER.0 + 3.0;
                // The two front dents of the Montana peak, shaded either side of the crease.
                let dent = dx.abs() < 3.0 - (fy - top) * 0.5 && fy < SHOULDER.0 + 2.5;
                *p = Some(if band {
                    dim(theme::ORANGE, if dx < 0.0 { 0.35 } else { 0.55 })
                } else if crease {
                    dim(theme::TEAL, 0.65)
                } else if dent {
                    dim(theme::TEAL, if dx < 0.0 { 0.3 } else { 0.5 })
                } else if dx < -half + 1.0 {
                    theme::CYAN
                } else if dx < 0.0 {
                    theme::TEAL
                } else {
                    dim(theme::TEAL, 0.35)
                });
                continue;
            }

            // Brim: the top surface, a bright front edge, and a sliver of shaded underside.
            if in_ellipse(fx, fy, by, brx, bry) {
                let edge = !in_ellipse(fx, fy + 1.0, by, brx, bry);
                *p = Some(if edge {
                    theme::CYAN
                } else {
                    dim(theme::TEAL, 0.2 + 0.25 * ((dx / brx) + 1.0) / 2.0)
                });
            } else if in_ellipse(fx, fy - 1.0, by, brx, bry) {
                *p = Some(dim(theme::TEAL, 0.7));
            }
        }
    }

    // Brass badge on the front of the crown, just above the band.
    let badge_y = cy + depth - 4.5;
    for (y, row) in px.iter_mut().enumerate() {
        for (x, p) in row.iter_mut().enumerate() {
            let (dx, dy) = (x as f32 + 0.5 - CX, y as f32 + 0.5 - badge_y);
            let r2 = dx * dx + dy * dy * 1.0;
            if r2 <= 0.8 {
                *p = Some(theme::ORANGE);
            } else if r2 <= 3.2 {
                *p = Some(theme::YELLOW);
            }
        }
    }
    px
}

/// Render pixel rows pairwise into half-block cells.
fn pixel_lines() -> Vec<Line<'static>> {
    let px = pixels();
    (0..H / 2)
        .map(|row| {
            let spans: Vec<Span> = (0..W)
                .map(|x| match (px[row * 2][x], px[row * 2 + 1][x]) {
                    (None, None) => Span::raw(" "),
                    (Some(t), None) => Span::styled("▀", theme::fg(t)),
                    (None, Some(b)) => Span::styled("▄", theme::fg(b)),
                    (Some(t), Some(b)) => Span::styled("▀", Style::new().fg(theme::c(t)).bg(theme::c(b))),
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// The GUMP logo with a horizontal sunset gradient on the solid blocks and muted shadows.
fn logo_lines() -> Vec<Line<'static>> {
    let width = LOGO[0].chars().count();
    LOGO.iter()
        .map(|l| {
            let spans: Vec<Span> = l
                .chars()
                .enumerate()
                .map(|(i, ch)| {
                    let style = if ch == '█' {
                        theme::fg(theme::gradient(&theme::SUNSET, i as f32 / width as f32))
                    } else {
                        theme::fg(theme::LINE)
                    };
                    Span::styled(ch.to_string(), style)
                })
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Banner lines that fit in `width` columns, with an info line underneath.
pub fn banner(width: u16, info: Vec<Span<'static>>) -> Vec<Line<'static>> {
    let quote = || {
        vec![
            Line::from(Span::styled(
                "“Gump! Why'd you put that together so quickly?”",
                theme::fg(theme::SOFT).italic(),
            )),
            Line::from(Span::styled("“You told me to, Drill Sergeant.”", theme::fg(theme::MUTED).italic())),
        ]
    };
    let logo_w = LOGO[0].chars().count();
    let width = width as usize;
    let gap = "   ";

    let mut out = if width >= W + gap.len() + logo_w {
        // Hat on the left; logo, the drill sergeant exchange and the info line on the right.
        // The hat fills its canvas top to bottom, so the logo lines up with the peak and the
        // info line with the brim.
        let mut right: Vec<Line> = logo_lines().into_iter().chain(quote()).chain([Line::from(info)]).collect();
        let pad = (H / 2).saturating_sub(right.len()) / 2;
        right.splice(0..0, std::iter::repeat_n(Line::raw(""), pad));
        let art = pixel_lines();
        let blank = Line::raw(" ".repeat(W));
        return (0..art.len().max(right.len()))
            .map(|i| {
                let mut spans = art.get(i).unwrap_or(&blank).spans.clone();
                spans.push(Span::raw(gap));
                spans.extend(right.get(i).map(|l| l.spans.clone()).unwrap_or_default());
                Line::from(spans)
            })
            .collect();
    } else if width >= logo_w {
        logo_lines().into_iter().chain(quote()).collect()
    } else {
        Vec::new()
    };
    out.push(Line::from(info));
    out
}

/// One of Forrest's lines for the end of a run.
pub fn sign_off(failed: bool, seed: usize) -> &'static str {
    const OK: [&str; 4] = [
        "That's all I have to say about that.",
        "Mama always said life was like a box of chocolates.",
        "And just like that, my running days were over.",
        "I'm pretty tired. I think I'll go home now.",
    ];
    if failed {
        "Stupid is as stupid does. Check the log."
    } else {
        OK[seed % OK.len()]
    }
}
