//! Theme loaded from `$MACUTIL_THEME`, `$XDG_CONFIG_HOME/macutil/theme.toml` or
//! `~/.config/macutil/theme.toml` (hand-edited, or rendered by matugen; see `matugen.rs`),
//! falling back to built-in Tokyo Night defaults. The file is re-read when its mtime changes, so wallpaper changes apply live.

use std::path::PathBuf;
use std::sync::RwLock;
use std::time::SystemTime;

use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Clone, Copy)]
struct Palette {
    bg: Color,
    fg: Color,
    dim: Color,
    accent: Color,
    on_accent: Color,
    good: Color,
    warn: Color,
    bad: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            bg: Color::Reset,
            fg: Color::Rgb(0xc0, 0xca, 0xf5),
            dim: Color::Rgb(0x56, 0x5f, 0x89),
            accent: Color::Rgb(0x7a, 0xa2, 0xf7),
            on_accent: Color::Black,
            good: Color::Rgb(0x9e, 0xce, 0x6a),
            warn: Color::Rgb(0xe0, 0xaf, 0x68),
            bad: Color::Rgb(0xf7, 0x76, 0x8e),
        }
    }
}

#[derive(Deserialize, Default)]
struct File {
    bg: Option<String>,
    fg: Option<String>,
    dim: Option<String>,
    accent: Option<String>,
    on_accent: Option<String>,
    good: Option<String>,
    warn: Option<String>,
    bad: Option<String>,
}

struct State {
    palette: Palette,
    mtime: Option<SystemTime>,
}

static STATE: RwLock<Option<State>> = RwLock::new(None);

fn path() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("MACUTIL_THEME") {
        return Some(p.into());
    }
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("macutil/theme.toml"))
}

fn hex(s: &Option<String>, fallback: Color) -> Color {
    let Some(s) = s else { return fallback };
    let h = s.trim().trim_start_matches('#');
    if h.len() != 6 {
        return fallback;
    }
    match u32::from_str_radix(h, 16) {
        Ok(v) => Color::Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8),
        Err(_) => fallback,
    }
}

/// `None` as the palette means the file exists but is unreadable or half-written;
/// the caller keeps the previous palette.
fn read() -> (Option<Palette>, Option<SystemTime>) {
    let d = Palette::default();
    let Some(p) = path() else { return (Some(d), None) };
    let mtime = std::fs::metadata(&p).and_then(|m| m.modified()).ok();
    let f: File = match std::fs::read_to_string(&p) {
        Ok(s) => match toml::from_str(&s) {
            Ok(f) => f,
            Err(_) => return (None, mtime),
        },
        Err(_) => File::default(),
    };
    let pal = Palette {
        bg: hex(&f.bg, d.bg),
        fg: hex(&f.fg, d.fg),
        dim: hex(&f.dim, d.dim),
        accent: hex(&f.accent, d.accent),
        on_accent: hex(&f.on_accent, d.on_accent),
        good: hex(&f.good, d.good),
        warn: hex(&f.warn, d.warn),
        bad: hex(&f.bad, d.bad),
    };
    (Some(pal), mtime)
}

/// Reloads the theme file if it changed. Cheap enough to call every frame.
pub fn refresh() {
    let current = STATE.read().ok().and_then(|s| s.as_ref().map(|s| s.mtime));
    let on_disk = path().and_then(|p| std::fs::metadata(p).and_then(|m| m.modified()).ok());
    if current == Some(on_disk) {
        return;
    }
    let (palette, mtime) = read();
    if let Ok(mut s) = STATE.write() {
        let palette = palette.or_else(|| s.as_ref().map(|s| s.palette)).unwrap_or_default();
        *s = Some(State { palette, mtime });
    }
}

fn pal() -> Palette {
    if STATE.read().map(|s| s.is_none()).unwrap_or(true) {
        refresh();
    }
    STATE.read().ok().and_then(|s| s.as_ref().map(|s| s.palette)).unwrap_or_default()
}

pub fn base() -> Style {
    let p = pal();
    Style::default().fg(p.fg).bg(p.bg)
}
pub fn dim() -> Style {
    Style::default().fg(pal().dim)
}
pub fn accent() -> Style {
    Style::default().fg(pal().accent).add_modifier(Modifier::BOLD)
}
pub fn selected() -> Style {
    let p = pal();
    Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)
}
pub fn good() -> Style {
    Style::default().fg(pal().good)
}
pub fn warn() -> Style {
    Style::default().fg(pal().warn)
}
pub fn bad() -> Style {
    Style::default().fg(pal().bad)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_parsing() {
        let fb = Color::Red;
        assert_eq!(hex(&Some("#7aa2f7".into()), fb), Color::Rgb(0x7a, 0xa2, 0xf7));
        assert_eq!(hex(&Some("nope".into()), fb), fb);
        assert_eq!(hex(&None, fb), fb);
    }
}
