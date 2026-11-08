//! City pop palette: a neon-teal night with sunset orange and yellow accents; pink and
//! purple are kept for small highlights only.
//! Uses 24-bit color when the terminal advertises it, otherwise the nearest xterm-256 color.

use ratatui::style::{Color, Modifier, Style};
use std::sync::OnceLock;

pub type Rgb = (u8, u8, u8);

pub const PINK: Rgb = (255, 106, 213);
pub const CYAN: Rgb = (121, 226, 242);
pub const TEAL: Rgb = (65, 226, 186);
pub const SKY: Rgb = (150, 200, 235);
pub const ORANGE: Rgb = (255, 170, 100);
pub const YELLOW: Rgb = (255, 230, 109);
pub const RED: Rgb = (255, 85, 120);
pub const TEXT: Rgb = (228, 240, 246);
pub const MUTED: Rgb = (108, 140, 160);
pub const LINE: Rgb = (52, 84, 108);
pub const NIGHT: Rgb = (22, 48, 66);
pub const INK: Rgb = (10, 24, 36);

/// The dominant UI color (borders, keys, headers), the secondary pill color, and soft text.
pub const PRIMARY: Rgb = CYAN;
pub const SECONDARY: Rgb = ORANGE;
pub const SOFT: Rgb = SKY;

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

/// Whether the terminal has a light background. `GUMP_THEME=light|dark` overrides detection.
fn light() -> bool {
    static LIGHT: OnceLock<bool> = OnceLock::new();
    *LIGHT.get_or_init(|| match std::env::var("GUMP_THEME").as_deref() {
        Ok("light") => true,
        Ok("dark") => false,
        _ => query_bg().map(|(r, g, b)| 0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32 > 128.0).unwrap_or_else(
            // COLORFGBG is "fg;bg" with ANSI color numbers; 7 and 9–15 are light backgrounds.
            || {
                std::env::var("COLORFGBG")
                    .ok()
                    .and_then(|v| v.rsplit(';').next()?.parse::<u8>().ok())
                    .is_some_and(|bg| bg == 7 || bg >= 9)
            },
        ),
    })
}

/// Detect the background before the wizard takes over the terminal.
pub fn init() {
    light();
}

/// Ask the terminal for its background color (OSC 11). None if it doesn't answer quickly.
#[cfg(unix)]
fn query_bg() -> Option<Rgb> {
    use std::io::{Read, Write};
    use std::os::fd::AsRawFd;
    use std::time::{Duration, Instant};
    let mut tty = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").ok()?;
    let fd = tty.as_raw_fd();
    // SAFETY: plain termios/select calls on a file descriptor we own; the old mode is restored.
    unsafe {
        let mut old: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut old) != 0 {
            return None;
        }
        let mut raw = old;
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(fd, libc::TCSANOW, &raw);
        let mut buf = Vec::new();
        if tty.write_all(b"\x1b]11;?\x1b\\").and_then(|_| tty.flush()).is_ok() {
            let deadline = Instant::now() + Duration::from_millis(200);
            while !buf.ends_with(b"\x07") && !buf.ends_with(b"\x1b\\") {
                let left = deadline.saturating_duration_since(Instant::now());
                if left.is_zero() {
                    break;
                }
                // select, not poll: poll doesn't work on ttys on macOS.
                let mut set: libc::fd_set = std::mem::zeroed();
                libc::FD_SET(fd, &mut set);
                let mut tv = libc::timeval { tv_sec: 0, tv_usec: left.as_micros() as _ };
                if libc::select(fd + 1, &mut set, std::ptr::null_mut(), std::ptr::null_mut(), &mut tv) <= 0 {
                    break;
                }
                let mut b = [0u8; 64];
                match tty.read(&mut b) {
                    Ok(n) if n > 0 => buf.extend_from_slice(&b[..n]),
                    _ => break,
                }
            }
        }
        libc::tcsetattr(fd, libc::TCSANOW, &old);
        parse_osc11(&String::from_utf8_lossy(&buf))
    }
}

#[cfg(not(unix))]
fn query_bg() -> Option<Rgb> {
    None
}

/// Parse `…rgb:RRRR/GGGG/BBBB…` (1–4 hex digits per channel).
fn parse_osc11(s: &str) -> Option<Rgb> {
    let rest = &s[s.find("rgb:")? + 4..];
    let mut ch = rest.split('/').map(|p| {
        let hex: String = p.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
        let max = (1u32 << (4 * hex.len() as u32)) - 1;
        u32::from_str_radix(&hex, 16).ok().filter(|_| !hex.is_empty()).map(|v| (v * 255 / max) as u8)
    });
    Some((ch.next()??, ch.next()??, ch.next()??))
}

/// Light terminals: flip each color's lightness and keep its hue, so bright-on-dark becomes
/// dark-on-light everywhere, gradients included.
fn for_light((r, g, b): Rgb) -> Rgb {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    let s = if d == 0.0 { 0.0 } else { d / (1.0 - (2.0 * l - 1.0).abs()) };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    // Accents land a bit darker than a plain flip so they read on white.
    let l = if s > 0.3 && l > 0.45 { (1.0 - l) * 0.65 } else { 1.0 - l };
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to(r), to(g), to(b))
}

pub fn c(rgb: Rgb) -> Color {
    let rgb = if light() { for_light(rgb) } else { rgb };
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

/// The signature sunset → sea sweep used for the logo and progress bar.
pub const SUNSET: [Rgb; 4] = [ORANGE, YELLOW, TEAL, CYAN];
