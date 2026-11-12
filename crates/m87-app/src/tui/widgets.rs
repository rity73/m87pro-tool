//! Small drawing helpers shared by the pages.

use m87_proto::layout::{Key, KEYS};
use m87_proto::lighting::Rgb;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use super::{Hit, Level, Target, PALETTE};

pub const DIM: Style = Style::new().fg(Color::DarkGray);
pub const LABEL: Style = Style::new().fg(Color::Gray);
pub const BTN: Style = Style::new().fg(Color::Black).bg(Color::Gray);
pub const BTN_ON: Style = Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD);
pub const BTN_MAIN: Style = Style::new().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD);
pub const BTN_WARN: Style = Style::new().fg(Color::Black).bg(Color::LightRed);

pub fn text_on(c: Rgb) -> Color {
    let luma = 299 * c.r as u32 + 587 * c.g as u32 + 114 * c.b as u32;
    if luma > 128_000 {
        Color::Black
    } else {
        Color::White
    }
}

pub fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

/// Draws inline items left to right, wrapping to the next line when
/// needed, and records click areas.
pub struct Row<'a> {
    pub x: u16,
    pub y: u16,
    left: u16,
    right: u16,
    bottom: u16,
    pub hits: &'a mut Vec<(Rect, Hit)>,
}

impl<'a> Row<'a> {
    pub fn new(area: Rect, y: u16, hits: &'a mut Vec<(Rect, Hit)>) -> Self {
        Row { x: area.x, y, left: area.x, right: area.right(), bottom: area.bottom(), hits }
    }

    /// Indent used after wrapping.
    pub fn indent(&mut self, cols: u16) -> &mut Self {
        self.left = (self.left + cols).min(self.right);
        self
    }

    pub fn text(&mut self, f: &mut Frame, s: &str, style: Style) -> &mut Self {
        if self.y >= self.bottom {
            return self;
        }
        let w = (s.chars().count() as u16).min(self.right.saturating_sub(self.x));
        if w > 0 {
            f.render_widget(Paragraph::new(Span::styled(s.to_string(), style)), Rect::new(self.x, self.y, w, 1));
        }
        self.x += w;
        self
    }

    pub fn button(&mut self, f: &mut Frame, s: &str, style: Style, hit: Option<Hit>) -> &mut Self {
        let w = s.chars().count() as u16;
        if self.x + w > self.right && self.x > self.left {
            self.newline();
        }
        let (start, y) = (self.x, self.y);
        self.text(f, s, style);
        if let Some(h) = hit {
            if y < self.bottom {
                self.hits.push((Rect::new(start, y, self.x - start, 1), h));
            }
        }
        self
    }

    pub fn gap(&mut self, n: u16) -> &mut Self {
        self.x = (self.x + n).min(self.right);
        self
    }

    pub fn newline(&mut self) -> &mut Self {
        self.y += 1;
        self.x = self.left;
        self
    }
}

pub fn color_controls(f: &mut Frame, hits: &mut Vec<(Rect, Hit)>, area: Rect, y: u16, target: Target, color: Rgb, enabled: bool) -> u16 {
    let label = if enabled { LABEL } else { DIM };
    let mut r = Row::new(area, y, hits);
    r.text(f, "Color      ", label).indent(11);
    for (i, (name, c)) in PALETTE.iter().enumerate() {
        // "Off" only makes sense for per-key colors.
        if target == Target::Effect && *c == Rgb::default() {
            continue;
        }
        let style = if enabled { Style::new().bg(rgb(*c)).fg(text_on(*c)) } else { DIM };
        let mark = if *c == color { "*" } else { " " };
        r.button(f, &format!("{mark}{name} "), style, enabled.then_some(Hit::Palette(target, i)));
    }
    let mut r = Row::new(area, r.y + 1, hits);
    r.text(f, "Custom     ", label);
    for (ch, name, v) in [(0, "Red", color.r), (1, "Green", color.g), (2, "Blue", color.b)] {
        r.text(f, &format!("{name} "), label)
            .button(f, " - ", if enabled { BTN } else { DIM }, enabled.then_some(Hit::Channel(target, ch, -16)))
            .button(f, &format!(" {v:>3} "), label, enabled.then_some(Hit::Channel(target, ch, 0)))
            .button(f, " + ", if enabled { BTN } else { DIM }, enabled.then_some(Hit::Channel(target, ch, 16)))
            .gap(2);
    }
    let preview = if enabled { Style::new().bg(rgb(color)).fg(text_on(color)) } else { DIM };
    r.text(f, &format!(" #{} ", color.to_hex()), preview);
    r.y + 1
}

#[allow(clippy::too_many_arguments)]
pub fn level_row(
    f: &mut Frame,
    hits: &mut Vec<(Rect, Hit)>,
    area: Rect,
    y: u16,
    name: &str,
    level: Level,
    value: u8,
    max: u8,
    enabled: bool,
    note: &str,
) {
    let label = if enabled { LABEL } else { DIM };
    let mut r = Row::new(area, y, hits);
    r.text(f, &format!("{name:<13}"), label);
    r.button(f, " - ", if enabled { BTN } else { DIM }, enabled.then_some(Hit::Step(level, -1))).gap(1);
    let bar: String = (1..=max).map(|i| if i <= value { '#' } else { '.' }).collect();
    r.button(f, &format!("{bar} {value}/{max}"), if enabled { Style::new().fg(Color::White) } else { DIM }, enabled.then_some(Hit::Step(level, 0)))
        .gap(1);
    r.button(f, " + ", if enabled { BTN } else { DIM }, enabled.then_some(Hit::Step(level, 1))).gap(2);
    r.text(f, note, DIM);
}

/// Area for the keyboard picture at the top of `a`. Every page that shows
/// the keyboard uses this, so the picture looks the same everywhere: keys
/// two lines tall when the window is tall enough, otherwise one.
pub fn keyboard_area(a: Rect) -> Rect {
    let h: u16 = if a.height >= 24 { 13 } else { 7 };
    Rect::new(a.x, a.y, a.width, h.min(a.height))
}

/// Screen rectangle of each key, scaled to fit `area`. Keys are two lines
/// tall when there is room, otherwise one.
pub fn key_rects(area: Rect) -> Vec<(&'static Key, Rect)> {
    let units = KEYS.iter().map(|k| (k.x4 + k.w4) as u16).max().unwrap_or(73);
    let scale = (area.width as f32 / units as f32).clamp(1.0, 1.6);
    let row_h: u16 = if area.height >= 6 * 2 + 1 { 2 } else { 1 };
    let total_w = (units as f32 * scale) as u16;
    let x0 = area.x + area.width.saturating_sub(total_w) / 2;
    KEYS.iter()
        .map(|k| {
            let x = x0 + (k.x4 as f32 * scale).round() as u16;
            let w = ((k.w4 as f32 * scale).round() as u16).saturating_sub(1).max(1);
            // Blank line under the function row.
            let y = area.y + k.row as u16 * row_h + if k.row > 0 { 1 } else { 0 };
            (k, Rect::new(x, y, w, row_h))
        })
        .filter(|(_, r)| r.bottom() <= area.bottom())
        .collect()
}

/// Draw one key cap.
pub fn key_cap(f: &mut Frame, rect: Rect, label: &str, style: Style) {
    let w = rect.width as usize;
    let label: String = label.chars().take(w).collect();
    let mut lines = vec![ratatui::text::Line::from(format!("{label:<w$}"))];
    for _ in 1..rect.height {
        lines.push(ratatui::text::Line::from(" ".repeat(w)));
    }
    f.render_widget(Paragraph::new(lines).style(style), rect);
}
