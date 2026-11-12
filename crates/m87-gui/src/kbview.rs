//! The keyboard picture: geometry, hit testing and GDI drawing.

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

/// Rectangle of every key inside a `w` x `h` area. The function row is
/// followed by half a row of space, as on the real keyboard.
pub fn key_rects(w: i32, h: i32) -> Vec<(&'static Key, RECT)> {
    let units = KEYS.iter().map(|k| (k.x4 + k.w4) as i32).max().unwrap_or(73);
    let margin = 6;
    let unit = (w - 2 * margin) as f32 / units as f32;
    let row_h = (h - 2 * margin) as f32 / 6.5;
    let gap = (unit * 0.35).clamp(2.0, 6.0);
    KEYS.iter()
        .map(|k| {
            let top = k.row as f32 + if k.row > 0 { 0.5 } else { 0.0 };
            let r = RECT {
                left: margin + (k.x4 as f32 * unit) as i32,
                top: margin + (top * row_h) as i32,
                right: margin + ((k.x4 + k.w4) as f32 * unit - gap) as i32,
                bottom: margin + ((top + 1.0) * row_h - gap) as i32,
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

const PANEL: Rgb = Rgb::new(0x20, 0x20, 0x24);
const KEY_OFF: Rgb = Rgb::new(0x3A, 0x3A, 0x40);
const LABEL_DIM: Rgb = Rgb::new(0xC8, 0xC8, 0xC8);
const CHANGED: Rgb = Rgb::new(0xFF, 0xD8, 0x30);
const SELECTED: Rgb = Rgb::new(0x30, 0xC8, 0xFF);
const CHOOSING: Rgb = Rgb::new(0x2D, 0x4F, 0x9E);

/// Draw the whole picture into `hdc` (already double buffered by the caller).
///
/// # Safety
/// `hdc` must be a valid device context and `font` a valid font.
pub unsafe fn paint(hdc: HDC, w: i32, h: i32, app: &App, mode: Mode, font: HFONT) {
    let all = RECT { left: 0, top: 0, right: w, bottom: h };
    let bg = CreateSolidBrush(colorref(PANEL));
    FillRect(hdc, &all, bg);
    let _ = DeleteObject(bg.into());

    let old_font = SelectObject(hdc, font.into());
    SetBkMode(hdc, TRANSPARENT);
    let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));
    let old_brush = SelectObject(hdc, GetStockObject(DC_BRUSH));
    let radius = (h / 40).max(4);
    let selected = app.map_key.map(|k| k.index);

    for (k, r) in key_rects(w, h) {
        let (fill, text) = match mode {
            Mode::Colors => {
                let c = app.key_colors[k.index as usize];
                if c == Rgb::default() {
                    (KEY_OFF, LABEL_DIM)
                } else {
                    (c, text_on(c))
                }
            }
            Mode::Mapping => {
                if selected == Some(k.index) {
                    (SELECTED, Rgb::new(0, 0, 0))
                } else if app.choosing_key {
                    (CHOOSING, Rgb::new(0xFF, 0xFF, 0xFF))
                } else if app.state.key(app.layer, k.id) != KeyAction::Default {
                    (CHANGED, Rgb::new(0, 0, 0))
                } else {
                    (KEY_OFF, LABEL_DIM)
                }
            }
        };
        SetDCBrushColor(hdc, colorref(fill));
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SetTextColor(hdc, colorref(text));
        let mut tr = RECT { left: r.left + 4, top: r.top + 2, right: r.right - 3, bottom: r.bottom - 2 };
        let mut label: Vec<u16> = k.label.encode_utf16().collect();
        DrawTextW(hdc, &mut label, &mut tr, DT_LEFT | DT_TOP | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX);
    }
    SelectObject(hdc, old_brush);
    SelectObject(hdc, old_pen);
    SelectObject(hdc, old_font);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry() {
        let rects = key_rects(940, 250);
        assert_eq!(rects.len(), KEYS.len());
        for (k, r) in &rects {
            assert!(r.right > r.left && r.bottom > r.top, "{}", k.id);
            assert!(r.right <= 940 && r.bottom <= 250, "{}", k.id);
        }
        // Clicking the middle of a key finds it.
        for (k, r) in &rects {
            assert_eq!(hit(940, 250, (r.left + r.right) / 2, (r.top + r.bottom) / 2), Some(k.index), "{}", k.id);
        }
        assert_eq!(hit(940, 250, 0, 0), None);
    }
}
