//! The banner printed above the wizard: Forrest's Monument Valley road at sunset next to the GUMP logo.
//! The skyline is pixel art drawn with half blocks (two pixels per terminal cell).

use super::theme::{self, Rgb};
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

const W: usize = 36;
const H: usize = 16;

const LOGO: [&str; 6] = [
    " ██████╗ ██╗   ██╗███╗   ███╗██████╗ ",
    "██╔════╝ ██║   ██║████╗ ████║██╔══██╗",
    "██║  ███╗██║   ██║██╔████╔██║██████╔╝",
    "██║   ██║██║   ██║██║╚██╔╝██║██╔═══╝ ",
    "╚██████╔╝╚██████╔╝██║ ╚═╝ ██║██║     ",
    " ╚═════╝  ╚═════╝ ╚═╝     ╚═╝╚═╝     ",
];

/// The Monument Valley road where Forrest stopped running: a striped sunset between the
/// buttes, the highway running to the horizon, and the white feather drifting overhead.
/// `None` is transparent.
fn skyline() -> [[Option<Rgb>; W]; H] {
    const HORIZON: usize = 11;
    let mut px = [[None; W]; H];
    // Striped retro sun sinking into the horizon.
    let (cx, cy, r) = (18.0f32, 11.5f32, 8.5f32);
    for (y, row) in px.iter_mut().enumerate().take(HORIZON) {
        if [6, 8, 10].contains(&y) {
            continue;
        }
        let color = theme::gradient(&[theme::YELLOW, theme::ORANGE, theme::PINK, theme::MAGENTA], y as f32 / 10.0);
        for (x, p) in row.iter_mut().enumerate() {
            let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);
            if dx * dx + dy * dy <= r * r {
                *p = Some(color);
            }
        }
    }
    // The mittens and spires: (left, right, top). Flat tops with a sunlit rim, flaring at the base.
    for (x0, x1, top) in [(2usize, 5usize, 4usize), (7, 7, 7), (28, 31, 3), (26, 26, 6), (34, 34, 8)] {
        for (y, row) in px.iter_mut().enumerate().take(HORIZON).skip(top) {
            let flare = usize::from(y + 2 >= HORIZON);
            for p in &mut row[x0.saturating_sub(flare)..=(x1 + flare).min(W - 1)] {
                *p = Some(if y == top { theme::PURPLE } else { theme::LINE });
            }
        }
    }
    // Horizon glow, then desert floor.
    for p in px[HORIZON].iter_mut() {
        *p = Some(theme::PINK);
    }
    for row in px.iter_mut().skip(HORIZON + 1) {
        row.fill(Some(theme::NIGHT));
    }
    // The highway, widening toward the viewer, with a dashed yellow center line.
    for (y, row) in px.iter_mut().enumerate().skip(HORIZON + 1) {
        let half = 1.0 + (y - HORIZON - 1) as f32 * 1.8;
        for (x, p) in row.iter_mut().enumerate() {
            let d = (x as f32 + 0.5 - 18.0).abs();
            if d <= half {
                *p = Some(if d < 1.0 && y % 2 == 1 && y > HORIZON + 1 { theme::YELLOW } else { theme::INK });
            }
        }
    }
    // The feather: a quill with its vane, drifting down on the left.
    for (x, y) in [(10, 0), (11, 0), (8, 1), (9, 1), (10, 1), (7, 2), (8, 2), (6, 3)] {
        px[y][x] = Some(theme::TEXT);
    }
    px
}

/// Render pixel rows pairwise into half-block cells.
fn pixel_lines() -> Vec<Line<'static>> {
    let px = skyline();
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
                theme::fg(theme::LAVENDER).italic(),
            )),
            Line::from(Span::styled("“You told me to, Drill Sergeant.”", theme::fg(theme::MUTED).italic())),
        ]
    };
    let logo_w = LOGO[0].chars().count();
    let width = width as usize;
    let gap = "   ";

    let mut out = if width >= W + gap.len() + logo_w {
        // Skyline on the left; logo and the drill sergeant exchange on the right.
        let right = logo_lines().into_iter().chain(quote());
        let mut v: Vec<Line> = pixel_lines()
            .into_iter()
            .zip(right)
            .map(|(art, text)| {
                let mut spans = art.spans;
                spans.push(Span::raw(gap));
                spans.extend(text.spans);
                Line::from(spans)
            })
            .collect();
        let mut info_line = vec![Span::raw(" ".repeat(W + gap.len()))];
        info_line.extend(info);
        v.push(Line::from(info_line));
        return v;
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
