//! `macutil panel-position <ghostty> <cols> <rows>`: where Macutil.app should place its Ghostty
//! panel so it opens centred. Ghostty can only be given a top-left position, and otherwise reopens
//! where its last window closed, so this predicts the window size the way Ghostty computes it:
//! each cell is the font's advance and line height rounded to whole device pixels, and the
//! window adds twice the configured padding on each axis (measured on Ghostty 1.3).
//! Prints `x y` in points from the top-left of the main screen's visible area, or nothing.

use std::ffi::c_void;
use std::process::{Command, Stdio};

use objc2::MainThreadMarker;
use objc2_app_kit::NSScreen;

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    fn CTFontCreateWithName(name: *const c_void, size: f64, matrix: *const c_void) -> *const c_void;
    fn CTFontCopyFamilyName(font: *const c_void) -> *const c_void;
    fn CTFontGetAscent(font: *const c_void) -> f64;
    fn CTFontGetDescent(font: *const c_void) -> f64;
    fn CTFontGetLeading(font: *const c_void) -> f64;
    fn CTFontGetGlyphsForCharacters(font: *const c_void, chars: *const u16, glyphs: *mut u16, count: isize) -> bool;
    fn CTFontGetAdvancesForGlyphs(font: *const c_void, orientation: u32, glyphs: *const u16, advances: *mut [f64; 2], count: isize) -> f64;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFStringCreateWithBytes(alloc: *const c_void, bytes: *const u8, len: isize, encoding: u32, external: bool) -> *const c_void;
    fn CFEqual(a: *const c_void, b: *const c_void) -> bool;
    fn CFRelease(o: *const c_void);
}

const UTF8: u32 = 0x0800_0100;

/// Ghostty's defaults, for keys the user hasn't set. JetBrains Mono is built into Ghostty, so
/// CoreText may not find it; its metrics (per em) are used instead.
const DEFAULT_SIZE: f64 = 13.0;
const DEFAULT_PADDING: f64 = 2.0;
const JETBRAINS_MONO: (f64, f64) = (0.6, 1.32);

struct Config {
    family: Option<String>,
    size: f64,
    /// Total padding (both sides) on each axis.
    pad_x: f64,
    pad_y: f64,
    adjust_w: Option<String>,
    adjust_h: Option<String>,
}

/// `20` means 20 on both sides, `10,14` means 10 and 14.
fn padding(v: &str) -> Option<f64> {
    let parts: Vec<f64> = v.split(',').filter_map(|p| p.trim().parse().ok()).collect();
    match parts[..] {
        [a] => Some(a * 2.0),
        [a, b] => Some(a + b),
        _ => None,
    }
}

fn read_config(ghostty: &str) -> Option<Config> {
    let out = Command::new(ghostty).arg("+show-config").stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    let mut c = Config {
        family: None,
        size: DEFAULT_SIZE,
        pad_x: DEFAULT_PADDING * 2.0,
        pad_y: DEFAULT_PADDING * 2.0,
        adjust_w: None,
        adjust_h: None,
    };
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Some((k, v)) = line.split_once(" = ") else { continue };
        let v = v.trim();
        match k {
            // The first family is the primary font; later ones are fallbacks.
            "font-family" if c.family.is_none() && !v.is_empty() => c.family = Some(v.trim_matches('"').to_string()),
            "font-size" => c.size = v.parse().unwrap_or(c.size),
            "window-padding-x" => c.pad_x = padding(v).unwrap_or(c.pad_x),
            "window-padding-y" => c.pad_y = padding(v).unwrap_or(c.pad_y),
            "adjust-cell-width" if !v.is_empty() => c.adjust_w = Some(v.to_string()),
            "adjust-cell-height" if !v.is_empty() => c.adjust_h = Some(v.to_string()),
            _ => {}
        }
    }
    Some(c)
}

fn cfstr(s: &str) -> *const c_void {
    unsafe { CFStringCreateWithBytes(std::ptr::null(), s.as_ptr(), s.len() as isize, UTF8, false) }
}

/// Advance of `M` and line height, in points, or `None` if the font isn't installed
/// (CoreText then substitutes another family, which would give the wrong size).
fn font_metrics(family: &str, size: f64) -> Option<(f64, f64)> {
    unsafe {
        let name = cfstr(family);
        let font = CTFontCreateWithName(name, size, std::ptr::null());
        let got = CTFontCopyFamilyName(font);
        let found = CFEqual(got, name);
        CFRelease(got);
        CFRelease(name);
        let mut glyph = 0u16;
        let mut adv = [[0.0f64; 2]];
        let ok = found && CTFontGetGlyphsForCharacters(font, &(b'M' as u16), &mut glyph, 1);
        if ok {
            CTFontGetAdvancesForGlyphs(font, 0, &glyph, adv.as_mut_ptr(), 1);
        }
        let height = CTFontGetAscent(font) + CTFontGetDescent(font) + CTFontGetLeading(font);
        CFRelease(font);
        ok.then_some((adv[0][0], height))
    }
}

/// Ghostty's `adjust-cell-*`: `N%` scales the cell, `N` adds N pixels.
fn adjust(px: f64, v: &Option<String>) -> f64 {
    match v.as_deref() {
        Some(v) if v.ends_with('%') => v.trim_end_matches('%').parse::<f64>().map(|p| px * (1.0 + p / 100.0)).unwrap_or(px),
        Some(v) => v.parse::<f64>().map(|d| px + d).unwrap_or(px),
        None => px,
    }
}

pub fn position(ghostty: &str, cols: f64, rows: f64) -> Option<(i64, i64)> {
    let c = read_config(ghostty)?;
    let screen = NSScreen::mainScreen(MainThreadMarker::new()?)?;
    let scale = screen.backingScaleFactor();
    let visible = screen.visibleFrame();
    let (adv, height) = c
        .family
        .as_deref()
        .and_then(|f| font_metrics(f, c.size))
        .unwrap_or((JETBRAINS_MONO.0 * c.size, JETBRAINS_MONO.1 * c.size));
    let cell_w = adjust((adv * scale).round(), &c.adjust_w).round() / scale;
    let cell_h = adjust((height * scale).round(), &c.adjust_h).round() / scale;
    let w = cols * cell_w + 2.0 * c.pad_x;
    let h = rows * cell_h + 2.0 * c.pad_y;
    let x = ((visible.size.width - w) / 2.0).max(0.0);
    let y = ((visible.size.height - h) / 2.0).max(0.0);
    Some((x.round() as i64, y.round() as i64))
}
