//! City pop palette: sunset pinks and oranges over a neon-lit night sky.
//! Uses 24-bit color when the terminal advertises it, otherwise the nearest xterm-256 color.

use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;

pub type Rgb = (u8, u8, u8);

pub const PINK: Rgb = (255, 106, 213);
pub const MAGENTA: Rgb = (255, 79, 154);
pub const PURPLE: Rgb = (199, 116, 232);
pub const LAVENDER: Rgb = (173, 140, 255);
pub const CYAN: Rgb = (121, 226, 242);
pub const TEAL: Rgb = (65, 226, 186);
pub const ORANGE: Rgb = (255, 170, 100);
pub const YELLOW: Rgb = (255, 230, 109);
pub const RED: Rgb = (255, 85, 120);
pub const TEXT: Rgb = (240, 230, 255);
pub const MUTED: Rgb = (128, 110, 168);
pub const LINE: Rgb = (88, 66, 128);
pub const NIGHT: Rgb = (52, 33, 84);
pub const INK: Rgb = (30, 18, 52);

fn truecolor() -> bool {
    static TC: OnceLock<bool> = OnceLock::new();
    *TC.get_or_init(|| {
        let var = |k: &str| std::env::var(k).unwrap_or_default().to_lowercase();
        let ct = var("COLORTERM");
        let prog = var("TERM_PROGRAM");
        let term = var("TERM");
        ct == "truecolor"
            || ct == "24bit"
            || ["iterm.app", "wezterm", "ghostty", "vscode"].contains(&prog.as_str())
            || ["kitty", "ghostty", "alacritty", "foot", "wezterm"].iter().any(|t| term.contains(t))
    })
}

/// Nearest color in the xterm-256 6×6×6 cube.
fn to_256((r, g, b): Rgb) -> u8 {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let idx =
        |v: u8| LEVELS.iter().enumerate().min_by_key(|(_, l)| (**l as i16 - v as i16).abs()).map_or(0, |(i, _)| i);
    16 + 36 * idx(r) as u8 + 6 * idx(g) as u8 + idx(b) as u8
}

pub fn c(rgb: Rgb) -> Color {
    if truecolor() {
        Color::Rgb(rgb.0, rgb.1, rgb.2)
    } else {
        Color::Indexed(to_256(rgb))
    }
}

pub fn fg(rgb: Rgb) -> Style {
    Style::new().fg(c(rgb))
}

pub fn bold(rgb: Rgb) -> Style {
    fg(rgb).add_modifier(Modifier::BOLD)
}

/// A colored "pill": dark text on a bright background.
pub fn pill(rgb: Rgb) -> Style {
    Style::new().fg(c(INK)).bg(c(rgb)).add_modifier(Modifier::BOLD)
}

/// The highlighted row in lists.
pub fn highlight() -> Style {
    Style::new().bg(c(NIGHT)).add_modifier(Modifier::BOLD)
}

/// Color at position `t` (0..=1) along a gradient through `stops`.
pub fn gradient(stops: &[Rgb], t: f32) -> Rgb {
    if stops.len() == 1 {
        return stops[0];
    }
    let t = t.clamp(0.0, 1.0) * (stops.len() - 1) as f32;
    let i = (t.floor() as usize).min(stops.len() - 2);
    let f = t - i as f32;
    let (a, b) = (stops[i], stops[i + 1]);
    let mix = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * f).round() as u8;
    (mix(a.0, b.0), mix(a.1, b.1), mix(a.2, b.2))
}

/// The signature sunset → neon sweep used for the logo and progress bar.
pub const SUNSET: [Rgb; 4] = [ORANGE, PINK, PURPLE, CYAN];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gradient_endpoints_and_cube() {
        assert_eq!(gradient(&SUNSET, 0.0), ORANGE);
        assert_eq!(gradient(&SUNSET, 1.0), CYAN);
        assert_eq!(to_256((0, 0, 0)), 16);
        assert_eq!(to_256((255, 255, 255)), 231);
    }
}
