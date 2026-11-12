//! The keyboard picture: geometry, hit testing and GDI drawing.
//!
//! Keys keep their real proportions (1u keys are square) and the picture is
//! centered in the control, so it never looks stretched. Keycaps are drawn
//! with a lighter top and a visible lower edge so they read as raised.

use m87_proto::keymap::KeyAction;
use m87_proto::layout::{Key, KEYS};
use m87_proto::lighting::Rgb;
use windows::Win32::Foundation::{COLORREF, RECT};
use windows::Win32::Graphics::Gdi::*;

use crate::app::App;

/// What the picture is used for.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    /// Paint per-key colors.
    Colors,
    /// Pick a key to change.
    Mapping,
}

pub fn colorref(c: Rgb) -> COLORREF {
    COLORREF(c.r as u32 | (c.g as u32) << 8 | (c.b as u32) << 16)
}

pub fn text_on(c: Rgb) -> Rgb {
    let luma = 299 * c.r as u32 + 587 * c.g as u32 + 114 * c.b as u32;
    if luma > 128_000 {
        Rgb::new(0, 0, 0)
    } else {
        Rgb::new(0xFF, 0xFF, 0xFF)
    }
}

/// Mix `a` toward `b` by `t` (0.0 = a, 1.0 = b).
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb::new(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b))
}

const WHITE: Rgb = Rgb::new(0xFF, 0xFF, 0xFF);
const BLACK: Rgb = Rgb::new(0, 0, 0);

// ---------------------------------------------------------------- geometry

/// Width of the key area in key units (the layout uses quarter units).
fn width_u() -> f32 {
    KEYS.iter().map(|k| (k.x4 + k.w4) as f32 / 4.0).fold(0.0, f32::max)
}

/// Gap between the function row and the number row, in key units.
const FN_GAP: f32 = 0.25;
/// Case border around the keys, in key units.
const BEZEL: f32 = 0.38;
/// Space between neighbouring keycaps, in key units (each side).
const KEY_GAP: f32 = 0.045;

/// Size of one key unit and the case rectangle, centered in `w` x `h`.
fn frame(w: i32, h: i32) -> (f32, RECT) {
    let cols = width_u() + 2.0 * BEZEL;
    let rows = 6.0 + FN_GAP + 2.0 * BEZEL;
    let unit = ((w - 4) as f32 / cols).min((h - 4) as f32 / rows).max(1.0);
    let (cw, ch) = ((cols * unit) as i32, (rows * unit) as i32);
    let left = (w - cw) / 2;
    let top = (h - ch) / 2;
    (unit, RECT { left, top, right: left + cw, bottom: top + ch })
}

/// Top of `row`, in key units from the first row.
fn row_top(row: f32) -> f32 {
    if row > 0.0 {
        row + FN_GAP
    } else {
        row
    }
}

/// Rectangle of every keycap inside a `w` x `h` area.
pub fn key_rects(w: i32, h: i32) -> Vec<(&'static Key, RECT)> {
    let (unit, case) = frame(w, h);
    let ox = case.left as f32 + BEZEL * unit;
    let oy = case.top as f32 + BEZEL * unit;
    let gap = KEY_GAP * unit;
    KEYS.iter()
        .map(|k| {
            let x = ox + k.x4 as f32 / 4.0 * unit;
            let y = oy + row_top(k.row as f32) * unit;
            let r = RECT {
                left: (x + gap).round() as i32,
                top: (y + gap).round() as i32,
                right: (x + k.w4 as f32 / 4.0 * unit - gap).round() as i32,
                bottom: (y + unit - gap).round() as i32,
            };
            (k, r)
        })
        .collect()
}

pub fn hit(w: i32, h: i32, x: i32, y: i32) -> Option<u8> {
    key_rects(w, h)
        .into_iter()
        .find(|(_, r)| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
        .map(|(k, _)| k.index)
}

// ---------------------------------------------------------------- colors

/// Keycap groups, colored like the two-tone keycap sets of the keyboard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Group {
    /// Letters, digits and punctuation.
    Alpha,
    /// Modifiers, F5-F8 and the navigation block.
    Modifier,
    /// Esc, Enter, Space and the arrows.
    Accent,
}

fn group(k: &Key) -> Group {
    match k.id {
        "Esc" | "Enter" | "Space" | "Up" | "Down" | "Left" | "Right" => Group::Accent,
        "F5" | "F6" | "F7" | "F8" | "PrintScreen" | "ScrollLock" | "Pause" | "Insert" | "Home" | "PgUp" | "Delete"
        | "End" | "PgDn" | "Backspace" | "Tab" | "CapsLock" | "LShift" | "RShift" | "LCtrl" | "LWin" | "LAlt"
        | "RAlt" | "Fn" | "Menu" | "RCtrl" => Group::Modifier,
        _ => Group::Alpha,
    }
}

/// Mapping page: a white case with white, blue-gray and lavender keycaps.
const CASE_LIGHT: Rgb = Rgb::new(0xF1, 0xF2, 0xF6);
const CASE_LIGHT_EDGE: Rgb = Rgb::new(0xD3, 0xD6, 0xDF);
const CAP_ALPHA: Rgb = Rgb::new(0xFC, 0xFC, 0xFD);
const CAP_MOD: Rgb = Rgb::new(0x93, 0xA9, 0xD6);
const CAP_ACCENT: Rgb = Rgb::new(0xA7, 0xA2, 0xDC);
const LABEL_DARK: Rgb = Rgb::new(0x3A, 0x3D, 0x46);
const CHANGED: Rgb = Rgb::new(0xFF, 0xD3, 0x4D);
const SELECTED: Rgb = Rgb::new(0x00, 0x78, 0xD4);
const CHOOSING: Rgb = Rgb::new(0xCF, 0xE3, 0xFA);

/// Key colors page: the same case; unlit keys are plain gray.
const CAP_OFF: Rgb = Rgb::new(0xDD, 0xDF, 0xE5);
const LABEL_OFF: Rgb = Rgb::new(0x86, 0x8A, 0x95);

/// Top color and label color of one keycap.
fn cap_colors(k: &Key, app: &App, mode: Mode, selected: Option<u8>) -> (Rgb, Rgb) {
    match mode {
        Mode::Colors => {
            let c = app.key_colors[k.index as usize];
            if c == Rgb::default() {
                (CAP_OFF, LABEL_OFF)
            } else {
                (c, text_on(c))
            }
        }
        Mode::Mapping => {
            if selected == Some(k.index) {
                (SELECTED, WHITE)
            } else if app.state.key(app.layer, k.id) != KeyAction::Default {
                (CHANGED, LABEL_DARK)
            } else if app.choosing_key {
                (CHOOSING, LABEL_DARK)
            } else {
                match group(k) {
                    Group::Alpha => (CAP_ALPHA, LABEL_DARK),
                    Group::Modifier => (CAP_MOD, WHITE),
                    Group::Accent => (CAP_ACCENT, WHITE),
                }
            }
        }
    }
}

// ---------------------------------------------------------------- drawing

unsafe fn round_rect(hdc: HDC, r: RECT, radius: i32, fill: Rgb, edge: Option<Rgb>) {
    let pen = CreatePen(PS_SOLID, 1, colorref(edge.unwrap_or(fill)));
    let old_pen = SelectObject(hdc, pen.into());
    SetDCBrushColor(hdc, colorref(fill));
    let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
    SelectObject(hdc, old_pen);
    let _ = DeleteObject(pen.into());
}

/// One keycap: a darker skirt with the lighter top face set back from it,
/// more at the bottom than at the top, so the key looks raised.
unsafe fn keycap(hdc: HDC, r: RECT, unit: f32, top: Rgb) {
    let side = mix(top, BLACK, 0.18);
    let edge = mix(top, BLACK, 0.3);
    let radius = (unit * 0.1).max(2.0) as i32;
    round_rect(hdc, r, radius, side, Some(edge));
    let face = RECT {
        left: r.left + (unit * 0.08).max(2.0) as i32,
        top: r.top + (unit * 0.04).max(1.0) as i32,
        right: r.right - (unit * 0.08).max(2.0) as i32,
        bottom: r.bottom - (unit * 0.15).max(3.0) as i32,
    };
    round_rect(hdc, face, (radius - 1).max(1), mix(top, WHITE, 0.06), None);
}

/// The light bar to the right of the Enter row, as on the real keyboard.
unsafe fn light_bar(hdc: HDC, case: RECT, unit: f32, mode: Mode) {
    let x0 = case.left as f32 + (BEZEL + 61.0 / 4.0 + 0.2) * unit;
    let x1 = case.left as f32 + (BEZEL + width_u() - 0.2) * unit;
    let cy = case.top as f32 + (BEZEL + row_top(3.0) + 0.5) * unit;
    let hh = (unit * 0.07).max(2.0);
    let r = RECT { left: x0 as i32, top: (cy - hh) as i32, right: x1 as i32, bottom: (cy + hh) as i32 };
    match mode {
        Mode::Mapping => round_rect(hdc, r, (hh as i32).max(1), Rgb::new(0xDD, 0xDF, 0xE6), Some(CASE_LIGHT_EDGE)),
        Mode::Colors => {
            // A rainbow strip in small steps.
            let steps = 24;
            let width = r.right - r.left;
            for i in 0..steps {
                let hue = i as f32 / steps as f32;
                let seg = RECT {
                    left: r.left + width * i / steps,
                    top: r.top,
                    right: r.left + width * (i + 1) / steps,
                    bottom: r.bottom,
                };
                let b = CreateSolidBrush(colorref(hue_color(hue)));
                FillRect(hdc, &seg, b);
                let _ = DeleteObject(b.into());
            }
        }
    }
}

fn hue_color(h: f32) -> Rgb {
    let f = |n: f32| {
        let k = (n + h * 6.0) % 6.0;
        let v = 1.0 - (k.min(4.0 - k).clamp(0.0, 1.0));
        (v * 255.0) as u8
    };
    Rgb::new(f(5.0), f(3.0), f(1.0))
}

/// Draw the whole picture into `hdc` (already double buffered by the caller).
/// `background` is the color around the keyboard.
///
/// # Safety
/// `hdc` must be a valid device context and `font` a valid font.
pub unsafe fn paint(hdc: HDC, w: i32, h: i32, app: &App, mode: Mode, font: HFONT, background: Rgb) {
    let all = RECT { left: 0, top: 0, right: w, bottom: h };
    let bg = CreateSolidBrush(colorref(background));
    FillRect(hdc, &all, bg);
    let _ = DeleteObject(bg.into());

    let (unit, case) = frame(w, h);
    // Legends sized to the keys, like printed keycaps.
    let mut lf = LOGFONTW::default();
    GetObjectW(font.into(), std::mem::size_of::<LOGFONTW>() as i32, Some(&mut lf as *mut _ as *mut core::ffi::c_void));
    lf.lfHeight = -((unit * 0.27).round() as i32).max(9);
    let legend = CreateFontIndirectW(&lf);
    let old_font = SelectObject(hdc, legend.into());
    SetBkMode(hdc, TRANSPARENT);
    let old_brush = SelectObject(hdc, GetStockObject(DC_BRUSH));

    round_rect(hdc, case, (unit * 0.3) as i32, CASE_LIGHT, Some(CASE_LIGHT_EDGE));
    light_bar(hdc, case, unit, mode);

    let selected = app.map_key.map(|k| k.index);
    for (k, r) in key_rects(w, h) {
        let (top, label) = cap_colors(k, app, mode, selected);
        keycap(hdc, r, unit, top);
        SetTextColor(hdc, colorref(label));
        // Centered on the top face, which sits above the lower edge.
        let mut tr = RECT { left: r.left + 1, top: r.top, right: r.right - 1, bottom: r.bottom - (unit * 0.11) as i32 };
        let mut text: Vec<u16> = legend_text(k).encode_utf16().collect();
        DrawTextW(hdc, &mut text, &mut tr, DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX);
    }
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_font);
    let _ = DeleteObject(legend.into());
}

/// Text printed on a keycap: the arrows show arrows, like the real keycaps.
fn legend_text(k: &Key) -> &'static str {
    match k.id {
        "Up" => "\u{2191}",
        "Down" => "\u{2193}",
        "Left" => "\u{2190}",
        "Right" => "\u{2192}",
        "CapsLock" => "Caps",
        _ => k.label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry() {
        for (w, h) in [(940, 250), (932, 272), (600, 400)] {
            let rects = key_rects(w, h);
            assert_eq!(rects.len(), KEYS.len());
            for (k, r) in &rects {
                assert!(r.right > r.left && r.bottom > r.top, "{}", k.id);
                assert!(r.left >= 0 && r.top >= 0 && r.right <= w && r.bottom <= h, "{}", k.id);
            }
            // Clicking the middle of a key finds it.
            for (k, r) in &rects {
                assert_eq!(hit(w, h, (r.left + r.right) / 2, (r.top + r.bottom) / 2), Some(k.index), "{}", k.id);
            }
            assert_eq!(hit(w, h, 0, 0), None);
        }
    }

    #[test]
    fn keys_are_square_whatever_the_area() {
        for (w, h) in [(940, 250), (932, 272), (1400, 250), (600, 400)] {
            let rects = key_rects(w, h);
            let (_, q) = rects.iter().find(|(k, _)| k.id == "Q").unwrap();
            let (qw, qh) = (q.right - q.left, q.bottom - q.top);
            assert!((qw - qh).abs() <= 1, "{w}x{h}: Q is {qw}x{qh}");
            // Space is 6.25 keys wide.
            let (_, s) = rects.iter().find(|(k, _)| k.id == "Space").unwrap();
            let ratio = (s.right - s.left) as f32 / (q.right - q.left) as f32;
            assert!((ratio - 6.9).abs() < 0.4, "{w}x{h}: space/Q = {ratio}");
        }
    }

    #[test]
    fn rainbow_starts_red() {
        assert_eq!(hue_color(0.0), Rgb::new(255, 0, 0));
    }
}
