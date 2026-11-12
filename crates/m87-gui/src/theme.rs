//! Windows 11 look with plain GDI: colors, fonts and the owner-drawn
//! controls (navigation items, buttons, color swatches and list items).
//! Nothing here needs the model, so drawing works even while it is borrowed.

use std::cell::RefCell;

use m87_proto::lighting::Rgb;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_NOFOCUSRECT, ODS_SELECTED};

use crate::kbview::colorref;

pub const BG: Rgb = Rgb::new(0xF3, 0xF3, 0xF3);
pub const CARD: Rgb = Rgb::new(0xFF, 0xFF, 0xFF);
pub const CARD_BORDER: Rgb = Rgb::new(0xE5, 0xE5, 0xE5);
pub const TEXT: Rgb = Rgb::new(0x1A, 0x1A, 0x1A);
pub const TEXT_DIM: Rgb = Rgb::new(0x5F, 0x5F, 0x5F);
pub const TEXT_OFF: Rgb = Rgb::new(0xA0, 0xA0, 0xA0);
pub const ACCENT: Rgb = Rgb::new(0x00, 0x5F, 0xB8);
const ACCENT_DOWN: Rgb = Rgb::new(0x00, 0x4C, 0x93);
const ACCENT_OFF: Rgb = Rgb::new(0xBF, 0xBF, 0xBF);
const BUTTON: Rgb = Rgb::new(0xFB, 0xFB, 0xFB);
const BUTTON_DOWN: Rgb = Rgb::new(0xF0, 0xF0, 0xF0);
const BUTTON_OFF: Rgb = Rgb::new(0xF5, 0xF5, 0xF5);
const BUTTON_BORDER: Rgb = Rgb::new(0xD0, 0xD0, 0xD0);
const NAV_ON: Rgb = Rgb::new(0xE6, 0xE6, 0xE6);
const NAV_DOWN: Rgb = Rgb::new(0xEC, 0xEC, 0xEC);
const LIST_ON: Rgb = Rgb::new(0xDD, 0xEB, 0xF7);
const LIST_ON_TEXT: Rgb = Rgb::new(0x00, 0x3E, 0x73);
pub const OK_TEXT: Rgb = Rgb::new(0x0F, 0x7B, 0x0F);
pub const ERR_TEXT: Rgb = Rgb::new(0xC4, 0x2B, 0x1C);
pub const NOTE_TEXT: Rgb = Rgb::new(0x9D, 0x5D, 0x00);

/// Fonts and brushes for the current DPI.
#[derive(Default)]
pub struct Theme {
    pub dpi: u32,
    pub font: HFONT,
    pub title: HFONT,
    pub icons: HFONT,
    pub bg: HBRUSH,
    pub card: HBRUSH,
}

thread_local! {
    pub static THEME: RefCell<Theme> = RefCell::new(Theme::default());
}

/// Scale a 96-DPI length.
pub fn s(v: i32) -> i32 {
    THEME.with(|t| v * t.borrow().dpi.max(96) as i32 / 96)
}

pub fn brush_bg() -> HBRUSH {
    THEME.with(|t| t.borrow().bg)
}

pub fn brush_card() -> HBRUSH {
    THEME.with(|t| t.borrow().card)
}

/// Create the brushes once; fonts come from `set_fonts`.
pub fn init() {
    THEME.with(|t| {
        let mut t = t.borrow_mut();
        unsafe {
            t.bg = CreateSolidBrush(colorref(BG));
            t.card = CreateSolidBrush(colorref(CARD));
        }
    });
}

/// Store the text font and make the title and icon fonts from it.
pub fn set_fonts(dpi: u32, base: &LOGFONTW, font: HFONT) {
    THEME.with(|t| {
        let mut t = t.borrow_mut();
        unsafe {
            for f in [t.title, t.icons] {
                if !f.is_invalid() {
                    let _ = DeleteObject(f.into());
                }
            }
            let mut title = *base;
            title.lfHeight = base.lfHeight * 3 / 2;
            title.lfWeight = 600;
            t.title = CreateFontIndirectW(&title);
            let mut icons = *base;
            icons.lfHeight = -(16 * dpi as i32 / 96);
            icons.lfWeight = 400;
            icons.lfFaceName = [0; 32];
            for (i, c) in "Segoe MDL2 Assets".encode_utf16().enumerate() {
                icons.lfFaceName[i] = c;
            }
            t.icons = CreateFontIndirectW(&icons);
        }
        t.dpi = dpi;
        t.font = font;
    });
}

pub fn title_font() -> HFONT {
    THEME.with(|t| t.borrow().title)
}

// ---------------------------------------------------------------- primitives

/// Rounded rectangle, filled, with an optional 1-pixel border.
pub fn round_rect(hdc: HDC, r: RECT, radius: i32, fill: Rgb, border: Option<Rgb>) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, colorref(border.unwrap_or(fill)));
        let brush = CreateSolidBrush(colorref(fill));
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, brush.into());
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius * 2, radius * 2);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(brush.into());
    }
}

fn ellipse(hdc: HDC, r: RECT, fill: Rgb, border: Rgb) {
    ellipse_w(hdc, r, fill, border, 1);
}

fn ellipse_w(hdc: HDC, r: RECT, fill: Rgb, border: Rgb, width: i32) {
    unsafe {
        let pen = CreatePen(PS_SOLID, width, colorref(border));
        let brush = CreateSolidBrush(colorref(fill));
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, brush.into());
        let _ = Ellipse(hdc, r.left, r.top, r.right, r.bottom);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(pen.into());
        let _ = DeleteObject(brush.into());
    }
}

/// GDI shapes have no anti-aliasing: draw `paint` at four times the size
/// and shrink it into `r` with averaging.
fn smooth(hdc: HDC, r: RECT, surface: Rgb, paint: impl Fn(HDC, i32)) {
    const K: i32 = 4;
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    if w <= 0 || h <= 0 {
        return;
    }
    unsafe {
        let mem = CreateCompatibleDC(Some(hdc));
        let bmp = CreateCompatibleBitmap(hdc, w * K, h * K);
        let old = SelectObject(mem, bmp.into());
        fill(mem, &RECT { left: 0, top: 0, right: w * K, bottom: h * K }, surface);
        paint(mem, K);
        SetStretchBltMode(hdc, HALFTONE);
        let _ = SetBrushOrgEx(hdc, 0, 0, None);
        let _ = StretchBlt(hdc, r.left, r.top, w, h, Some(mem), 0, 0, w * K, h * K, SRCCOPY);
        SelectObject(mem, old);
        let _ = DeleteObject(bmp.into());
        let _ = DeleteDC(mem);
    }
}

fn scaled(r: RECT, k: i32) -> RECT {
    RECT { left: r.left * k, top: r.top * k, right: r.right * k, bottom: r.bottom * k }
}

fn fill(hdc: HDC, r: &RECT, c: Rgb) {
    unsafe {
        let b = CreateSolidBrush(colorref(c));
        FillRect(hdc, r, b);
        let _ = DeleteObject(b.into());
    }
}

fn text(hdc: HDC, s: &str, mut r: RECT, color: Rgb, font: HFONT, flags: DRAW_TEXT_FORMAT) {
    unsafe {
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colorref(color));
        let old = SelectObject(hdc, font.into());
        let mut w: Vec<u16> = s.encode_utf16().collect();
        DrawTextW(hdc, &mut w, &mut r, flags | DT_SINGLELINE | DT_VCENTER | DT_NOPREFIX | DT_END_ELLIPSIS);
        SelectObject(hdc, old);
    }
}

fn inset(r: RECT, d: i32) -> RECT {
    RECT { left: r.left + d, top: r.top + d, right: r.right - d, bottom: r.bottom - d }
}

fn light(c: Rgb) -> bool {
    299 * c.r as u32 + 587 * c.g as u32 + 114 * c.b as u32 > 200_000
}

/// The white card behind the controls of a page.
pub fn card(hdc: HDC, r: RECT) {
    round_rect(hdc, r, s(8), CARD, Some(CARD_BORDER));
}

// ---------------------------------------------------------------- owner-drawn controls

pub enum Look<'a> {
    /// Accent-filled main action ("Apply to keyboard").
    Primary,
    /// Ordinary button.
    Secondary,
    /// Button with a color dot before its text ("Custom...").
    ColorButton(Rgb),
    /// Round color swatch; `selected` draws a ring around it.
    Swatch { color: Rgb, selected: bool },
    /// Item of the navigation column.
    Nav { glyph: &'a str, selected: bool },
}

struct State {
    disabled: bool,
    down: bool,
    focus: bool,
}

fn state(dis: &DRAWITEMSTRUCT) -> State {
    let st = dis.itemState.0;
    State {
        disabled: st & ODS_DISABLED.0 != 0,
        down: st & ODS_SELECTED.0 != 0,
        focus: st & ODS_FOCUS.0 != 0 && st & ODS_NOFOCUSRECT.0 == 0,
    }
}

/// Draw a button whose background is `surface` (card or window color).
pub fn draw_button(dis: &DRAWITEMSTRUCT, look: Look, label: &str, surface: Rgb) {
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let st = state(dis);
    let font = THEME.with(|t| t.borrow().font);
    fill(hdc, &r, surface);
    match look {
        Look::Primary => {
            let bg = if st.disabled {
                ACCENT_OFF
            } else if st.down {
                ACCENT_DOWN
            } else {
                ACCENT
            };
            round_rect(hdc, r, s(4), bg, None);
            text(hdc, label, r, Rgb::new(0xFF, 0xFF, 0xFF), font, DT_CENTER);
        }
        Look::Secondary | Look::ColorButton(_) => {
            let bg = if st.disabled {
                BUTTON_OFF
            } else if st.down {
                BUTTON_DOWN
            } else {
                BUTTON
            };
            round_rect(hdc, r, s(4), bg, Some(BUTTON_BORDER));
            let fg = if st.disabled { TEXT_OFF } else { TEXT };
            if let Look::ColorButton(c) = look {
                let d = s(14);
                let cy = (r.top + r.bottom) / 2;
                let dot = RECT { left: r.left + s(10), top: cy - d / 2, right: r.left + s(10) + d, bottom: cy - d / 2 + d };
                let dot_fill = if st.disabled { BUTTON_BORDER } else { c };
                let ring = if light(dot_fill) { BUTTON_BORDER } else { dot_fill };
                let local = RECT { left: 0, top: 0, right: d, bottom: d };
                smooth(hdc, dot, bg, |m, k| ellipse_w(m, scaled(local, k), dot_fill, ring, k));
                let tr = RECT { left: dot.right + s(8), ..r };
                text(hdc, label, tr, fg, font, DT_LEFT);
            } else {
                text(hdc, label, r, fg, font, DT_CENTER);
            }
        }
        Look::Swatch { color, selected } => {
            let size = (r.right - r.left).min(r.bottom - r.top);
            let outer = RECT {
                left: (r.left + r.right - size) / 2,
                top: (r.top + r.bottom - size) / 2,
                right: (r.left + r.right + size) / 2,
                bottom: (r.top + r.bottom + size) / 2,
            };
            let c = if st.disabled { Rgb::new(0xE4, 0xE4, 0xE4) } else { color };
            let ring = selected && !st.disabled;
            let focus = st.focus && !selected;
            let local = RECT { left: 0, top: 0, right: size, bottom: size };
            smooth(hdc, outer, surface, |m, k| {
                let big = scaled(local, k);
                if ring {
                    ellipse(m, big, ACCENT, ACCENT);
                    let gap = scaled(inset(local, s(2)), k);
                    ellipse(m, gap, surface, surface);
                } else if focus {
                    ellipse_w(m, inset(big, k / 2), surface, TEXT_DIM, k);
                }
                let dot = scaled(inset(local, s(4)), k);
                ellipse_w(m, dot, c, if light(c) { BUTTON_BORDER } else { c }, k);
            });
            return;
        }
        Look::Nav { glyph, selected } => {
            let item = RECT { left: r.left, top: r.top + s(2), right: r.right, bottom: r.bottom - s(2) };
            if selected || st.down {
                round_rect(hdc, item, s(4), if st.down { NAV_DOWN } else { NAV_ON }, None);
            }
            if selected {
                let cy = (r.top + r.bottom) / 2;
                let bar = RECT { left: r.left, top: cy - s(8), right: r.left + s(3), bottom: cy + s(8) };
                round_rect(hdc, bar, s(2), ACCENT, None);
            }
            let icons = THEME.with(|t| t.borrow().icons);
            let ir = RECT { left: r.left + s(12), right: r.left + s(34), ..r };
            text(hdc, glyph, ir, TEXT, icons, DT_CENTER);
            let lr = RECT { left: r.left + s(44), right: r.right - s(6), ..r };
            text(hdc, label, lr, TEXT, font, DT_LEFT);
        }
    }
    if st.focus {
        focus_ring(hdc, r);
    }
}

/// Keyboard focus: a thin dark outline just inside the control.
fn focus_ring(hdc: HDC, r: RECT) {
    unsafe {
        let pen = CreatePen(PS_SOLID, 1, colorref(TEXT));
        let old_pen = SelectObject(hdc, pen.into());
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let r = inset(r, 1);
        let rad = s(4) * 2;
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, rad, rad);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_brush);
        let _ = DeleteObject(pen.into());
    }
}

/// One row of an owner-drawn list box.
pub fn draw_list_item(dis: &DRAWITEMSTRUCT, label: &str, enabled: bool) {
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let selected = dis.itemState.0 & ODS_SELECTED.0 != 0;
    let font = THEME.with(|t| t.borrow().font);
    fill(hdc, &r, CARD);
    let fg = if !enabled {
        TEXT_OFF
    } else if selected {
        round_rect(hdc, RECT { left: r.left + s(3), top: r.top + s(1), right: r.right - s(3), bottom: r.bottom - s(1) }, s(4), LIST_ON, None);
        LIST_ON_TEXT
    } else {
        TEXT
    };
    let tr = RECT { left: r.left + s(12), right: r.right - s(8), ..r };
    text(hdc, label, tr, fg, font, DT_LEFT);
}

/// Glyphs of the "Segoe MDL2 Assets" font used in the navigation column.
pub mod glyph {
    pub const LIGHTING: &str = "\u{E82F}";
    pub const KEY_COLORS: &str = "\u{E790}";
    pub const KEYS: &str = "\u{E765}";
    pub const MACROS: &str = "\u{E7C8}";
    pub const SETTINGS: &str = "\u{E713}";
    pub const PROFILES: &str = "\u{E8B7}";
    pub const ABOUT: &str = "\u{E946}";
}
