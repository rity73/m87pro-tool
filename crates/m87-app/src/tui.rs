//! Full-screen terminal interface with mouse support.
//!
//! Every clickable element registers its screen rectangle in `hits` while it
//! is drawn; mouse events are matched against that list.

use std::io::stdout;
use std::time::{Duration, Instant};

use anyhow::Result;
use m87_hid::{list_interfaces, Keyboard, Role};
use m87_proto::layout::{Key, KEYS, SLOTS};
use m87_proto::lighting::{Effect, Lighting, Rgb, MAX_LEVEL};
use m87_proto::plan::{self, Packet};
use m87_proto::Link;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph};
use ratatui::{DefaultTerminal, Frame};

const MIN_W: u16 = 80;
const MIN_H: u16 = 24;

const PALETTE: [(&str, Rgb); 10] = [
    ("Red", Rgb::new(0xFF, 0x00, 0x00)),
    ("Orange", Rgb::new(0xFF, 0x80, 0x00)),
    ("Yellow", Rgb::new(0xFF, 0xFF, 0x00)),
    ("Green", Rgb::new(0x00, 0xFF, 0x00)),
    ("Cyan", Rgb::new(0x00, 0xFF, 0xFF)),
    ("Blue", Rgb::new(0x00, 0x00, 0xFF)),
    ("Purple", Rgb::new(0x80, 0x00, 0xFF)),
    ("Pink", Rgb::new(0xFF, 0x00, 0xFF)),
    ("White", Rgb::new(0xFF, 0xFF, 0xFF)),
    ("Off", Rgb::new(0x00, 0x00, 0x00)),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Lighting,
    KeyColors,
    Device,
}

impl Tab {
    const ALL: [Tab; 3] = [Tab::Lighting, Tab::KeyColors, Tab::Device];
    fn title(self) -> &'static str {
        match self {
            Tab::Lighting => "1 Lighting effect",
            Tab::KeyColors => "2 Per-key colors",
            Tab::Device => "3 Device",
        }
    }
}

/// Which color the palette and RGB controls edit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Effect,
    Brush,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Level {
    Brightness,
    Speed,
    Direction,
    KeyBrightness,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Hit {
    Tab(Tab),
    Effect(usize),
    Palette(Target, usize),
    Channel(Target, usize, i16),
    Rainbow,
    Step(Level, i8),
    Apply,
    Key(u8),
    FillAll,
    AllOff,
    Recheck,
    Quit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tone {
    Info,
    Ok,
    Err,
}

struct App {
    tab: Tab,
    lighting: Lighting,
    key_colors: [Rgb; SLOTS],
    brush: Rgb,
    key_brightness: u8,
    effect_scroll: usize,
    status: (Tone, String),
    link: Option<Link>,
    battery: Option<u8>,
    last_scan: Instant,
    pending_apply: bool,
    hits: Vec<(Rect, Hit)>,
    quit: bool,
}

impl App {
    fn new() -> Self {
        let mut app = App::blank();
        app.scan(true);
        app
    }

    fn blank() -> Self {
        App {
            tab: Tab::Lighting,
            lighting: Lighting::default(),
            key_colors: [Rgb::default(); SLOTS],
            brush: Rgb::new(0xFF, 0, 0),
            key_brightness: MAX_LEVEL,
            effect_scroll: 0,
            status: (Tone::Info, "Choose settings, then click Apply.".into()),
            link: None,
            battery: None,
            last_scan: Instant::now() - Duration::from_secs(60),
            pending_apply: false,
            hits: Vec::new(),
            quit: false,
        }
    }

    /// Detect the connection; read the battery when asked and wireless.
    fn scan(&mut self, read_battery: bool) {
        self.last_scan = Instant::now();
        let link = list_interfaces().iter().find(|i| i.role == Role::Config).map(|i| i.link);
        if link != self.link {
            self.battery = None;
        }
        self.link = link;
        if read_battery && link == Some(Link::Dongle) {
            self.battery = Keyboard::open().and_then(|kb| kb.battery()).ok();
        }
    }

    fn color_mut(&mut self, t: Target) -> &mut Rgb {
        match t {
            Target::Effect => &mut self.lighting.color,
            Target::Brush => &mut self.brush,
        }
    }

    fn packets(&self, link: Link) -> Vec<Packet> {
        match self.tab {
            Tab::KeyColors => {
                let colors: Vec<(u8, Rgb)> = KEYS.iter().map(|k| (k.index, self.key_colors[k.index as usize])).collect();
                plan::set_key_colors(link, self.key_brightness, &colors)
            }
            _ => plan::set_effect(link, &self.lighting),
        }
    }

    fn apply_now(&mut self) {
        let result = Keyboard::open().and_then(|kb| {
            let link = kb.link();
            kb.run(&self.packets(link)).map(|_| link)
        });
        self.status = match result {
            Ok(link) => {
                self.link = Some(link);
                let what = match self.tab {
                    Tab::KeyColors => "Per-key colors".to_string(),
                    _ => format!("Effect \"{}\"", self.lighting.effect.name()),
                };
                (Tone::Ok, format!("Applied. {what} sent over the {}.", link.label()))
            }
            Err(e) => (Tone::Err, format!("Not applied: {e}.")),
        };
    }

    fn step(&mut self, level: Level, delta: i8) {
        let (v, max) = match level {
            Level::Brightness => (&mut self.lighting.brightness, MAX_LEVEL),
            Level::Speed => (&mut self.lighting.speed, MAX_LEVEL),
            Level::Direction => (&mut self.lighting.direction, 3),
            Level::KeyBrightness => (&mut self.key_brightness, MAX_LEVEL),
        };
        *v = (*v as i16 + delta as i16).clamp(0, max as i16) as u8;
    }

    fn select_effect(&mut self, i: usize) {
        if let Some(e) = Effect::ALL.get(i) {
            self.lighting.effect = *e;
        }
    }

    fn on_hit(&mut self, hit: Hit) {
        match hit {
            Hit::Tab(t) => self.tab = t,
            Hit::Effect(i) => self.select_effect(i),
            Hit::Palette(t, i) => {
                *self.color_mut(t) = PALETTE[i].1;
                if t == Target::Effect {
                    self.lighting.rainbow = false;
                }
            }
            Hit::Channel(t, ch, d) => {
                let c = self.color_mut(t);
                let v = match ch {
                    0 => &mut c.r,
                    1 => &mut c.g,
                    _ => &mut c.b,
                };
                *v = (*v as i16 + d).clamp(0, 255) as u8;
                if t == Target::Effect {
                    self.lighting.rainbow = false;
                }
            }
            Hit::Rainbow => self.lighting.rainbow = !self.lighting.rainbow,
            Hit::Step(l, d) => self.step(l, d),
            Hit::Apply => {
                self.status = (Tone::Info, "Sending to the keyboard...".into());
                self.pending_apply = true;
            }
            Hit::Key(i) => self.key_colors[i as usize] = self.brush,
            Hit::FillAll => {
                for k in KEYS {
                    self.key_colors[k.index as usize] = self.brush;
                }
            }
            Hit::AllOff => self.key_colors = [Rgb::default(); SLOTS],
            Hit::Recheck => {
                self.scan(true);
                self.status = match self.link {
                    Some(l) => (Tone::Ok, format!("Keyboard found ({}).", l.label())),
                    None => (Tone::Err, "No keyboard found. Connect the USB cable or the 2.4 GHz receiver.".into()),
                };
            }
            Hit::Quit => self.quit = true,
        }
    }

    fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
        let p = Position::new(x, y);
        self.hits.iter().rev().find(|(r, _)| r.contains(p)).map(|(_, h)| *h)
    }

    fn on_scroll(&mut self, x: u16, y: u16, up: bool) {
        let d: i8 = if up { 1 } else { -1 };
        match self.hit_at(x, y) {
            Some(Hit::Effect(_)) => {
                let i = self.lighting.effect.id() as i16 - d as i16;
                self.select_effect(i.clamp(0, Effect::ALL.len() as i16 - 1) as usize);
            }
            Some(Hit::Step(l, _)) => self.step(l, d),
            Some(Hit::Channel(t, ch, _)) => self.on_hit(Hit::Channel(t, ch, d as i16 * 8)),
            _ => {}
        }
    }

    fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        match code {
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Char('1') => self.tab = Tab::Lighting,
            KeyCode::Char('2') => self.tab = Tab::KeyColors,
            KeyCode::Char('3') => self.tab = Tab::Device,
            KeyCode::Tab => {
                let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
                self.tab = Tab::ALL[(i + 1) % Tab::ALL.len()];
            }
            KeyCode::BackTab => {
                let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
                self.tab = Tab::ALL[(i + Tab::ALL.len() - 1) % Tab::ALL.len()];
            }
            KeyCode::Enter | KeyCode::Char('a') if self.tab != Tab::Device => self.on_hit(Hit::Apply),
            KeyCode::Up if self.tab == Tab::Lighting => {
                let i = self.lighting.effect.id() as usize;
                self.select_effect(i.saturating_sub(1));
            }
            KeyCode::Down if self.tab == Tab::Lighting => {
                let i = self.lighting.effect.id() as usize;
                self.select_effect((i + 1).min(Effect::ALL.len() - 1));
            }
            KeyCode::Left => {
                let l = if self.tab == Tab::KeyColors { Level::KeyBrightness } else { Level::Brightness };
                self.step(l, -1);
            }
            KeyCode::Right => {
                let l = if self.tab == Tab::KeyColors { Level::KeyBrightness } else { Level::Brightness };
                self.step(l, 1);
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------- drawing

fn text_on(c: Rgb) -> Color {
    let luma = 299 * c.r as u32 + 587 * c.g as u32 + 114 * c.b as u32;
    if luma > 128_000 {
        Color::Black
    } else {
        Color::White
    }
}

fn rgb(c: Rgb) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

const DIM: Style = Style::new().fg(Color::DarkGray);
const LABEL: Style = Style::new().fg(Color::Gray);
const BTN: Style = Style::new().fg(Color::Black).bg(Color::Gray);
const BTN_MAIN: Style = Style::new().fg(Color::Black).bg(Color::LightGreen).add_modifier(Modifier::BOLD);

/// Draws a row of inline items left to right and records click areas.
struct Row<'a> {
    x: u16,
    y: u16,
    right: u16,
    hits: &'a mut Vec<(Rect, Hit)>,
}

impl<'a> Row<'a> {
    fn new(area: Rect, y: u16, hits: &'a mut Vec<(Rect, Hit)>) -> Self {
        Row { x: area.x, y, right: area.right(), hits }
    }

    fn text(&mut self, f: &mut Frame, s: &str, style: Style) -> &mut Self {
        let w = (s.chars().count() as u16).min(self.right.saturating_sub(self.x));
        if w > 0 {
            f.render_widget(Paragraph::new(Span::styled(s.to_string(), style)), Rect::new(self.x, self.y, w, 1));
        }
        self.x += w;
        self
    }

    fn button(&mut self, f: &mut Frame, s: &str, style: Style, hit: Option<Hit>) -> &mut Self {
        let start = self.x;
        self.text(f, s, style);
        if let Some(h) = hit {
            self.hits.push((Rect::new(start, self.y, self.x - start, 1), h));
        }
        self
    }

    fn gap(&mut self, n: u16) -> &mut Self {
        self.x = (self.x + n).min(self.right);
        self
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    app.hits.clear();
    let area = f.area();
    if area.width < MIN_W || area.height < MIN_H {
        let msg = format!(
            "The window is too small ({}x{}). Make it at least {MIN_W}x{MIN_H}. Press Q to quit.",
            area.width, area.height
        );
        f.render_widget(Paragraph::new(msg).wrap(ratatui::widgets::Wrap { trim: true }), area);
        return;
    }
    let [top, tabs, body, status, help] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(1),
        Constraint::Min(10),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .areas(area);

    // Title and connection.
    let conn = match (app.link, app.battery) {
        (Some(Link::Dongle), Some(b)) => format!("Connected: 2.4 GHz receiver   Battery: {b}%"),
        (Some(l), _) => format!("Connected: {}", l.label()),
        (None, _) => "Not connected".to_string(),
    };
    let conn_style = if app.link.is_some() { Style::new().fg(Color::LightGreen) } else { Style::new().fg(Color::LightRed) };
    let mut r = Row::new(top, top.y, &mut app.hits);
    r.text(f, " M87 Pro ", Style::new().add_modifier(Modifier::BOLD).fg(Color::Black).bg(Color::Cyan))
        .gap(2)
        .text(f, &conn, conn_style);
    let quit = Rect::new(top.right().saturating_sub(8), top.y, 8, 1);
    f.render_widget(Paragraph::new(Span::styled(" Quit X ", BTN)), quit);
    app.hits.push((quit, Hit::Quit));

    // Tabs.
    let mut r = Row::new(tabs, tabs.y, &mut app.hits);
    for t in Tab::ALL {
        let style = if t == app.tab {
            Style::new().fg(Color::Black).bg(Color::White).add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(Color::Gray).bg(Color::DarkGray)
        };
        r.button(f, &format!(" {} ", t.title()), style, Some(Hit::Tab(t))).gap(1);
    }

    match app.tab {
        Tab::Lighting => draw_lighting(f, app, body),
        Tab::KeyColors => draw_key_colors(f, app, body),
        Tab::Device => draw_device(f, app, body),
    }

    let tone = match app.status.0 {
        Tone::Info => Style::new().fg(Color::White),
        Tone::Ok => Style::new().fg(Color::LightGreen),
        Tone::Err => Style::new().fg(Color::LightRed),
    };
    f.render_widget(Paragraph::new(Span::styled(format!(" {}", app.status.1), tone)), status);
    f.render_widget(
        Paragraph::new(Span::styled(
            " Click/drag/scroll with the mouse.  Keys: 1-3 page, Up/Down effect, Left/Right brightness, Enter apply, Q quit",
            DIM,
        )),
        help,
    );
}

fn color_controls(f: &mut Frame, hits: &mut Vec<(Rect, Hit)>, area: Rect, y: u16, target: Target, color: Rgb, enabled: bool) -> u16 {
    let label = if enabled { LABEL } else { DIM };
    // Palette swatches.
    let mut r = Row::new(area, y, hits);
    r.text(f, "Color      ", label);
    for (i, (name, c)) in PALETTE.iter().enumerate() {
        // "Off" only makes sense for per-key colors.
        if target == Target::Effect && *c == Rgb::default() {
            continue;
        }
        let style = if enabled { Style::new().bg(rgb(*c)).fg(text_on(*c)) } else { DIM };
        let mark = if *c == color { "*" } else { " " };
        r.button(f, &format!("{mark}{name} "), style, enabled.then_some(Hit::Palette(target, i)));
    }
    // Custom RGB.
    let mut r = Row::new(area, y + 1, hits);
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
    y + 2
}

fn level_row(f: &mut Frame, hits: &mut Vec<(Rect, Hit)>, area: Rect, y: u16, name: &str, level: Level, value: u8, max: u8, enabled: bool, note: &str) {
    let label = if enabled { LABEL } else { DIM };
    let mut r = Row::new(area, y, hits);
    r.text(f, &format!("{name:<11}"), label);
    r.button(f, " - ", if enabled { BTN } else { DIM }, enabled.then_some(Hit::Step(level, -1))).gap(1);
    let bar: String = (1..=max).map(|i| if i <= value { '#' } else { '.' }).collect();
    r.button(f, &format!("{bar} {value}/{max}"), if enabled { Style::new().fg(Color::White) } else { DIM }, enabled.then_some(Hit::Step(level, 0)))
        .gap(1);
    r.button(f, " + ", if enabled { BTN } else { DIM }, enabled.then_some(Hit::Step(level, 1))).gap(2);
    r.text(f, note, DIM);
}

fn draw_lighting(f: &mut Frame, app: &mut App, body: Rect) {
    let [list_area, settings_area] = Layout::horizontal([Constraint::Length(22), Constraint::Min(40)]).areas(body);

    // Effect list with scrolling.
    let block = Block::bordered().title(" Effect ");
    let inner = block.inner(list_area);
    f.render_widget(block, list_area);
    let rows = inner.height as usize;
    let sel = app.lighting.effect.id() as usize;
    if sel < app.effect_scroll {
        app.effect_scroll = sel;
    } else if sel >= app.effect_scroll + rows {
        app.effect_scroll = sel + 1 - rows;
    }
    for (row, i) in (app.effect_scroll..Effect::ALL.len()).take(rows).enumerate() {
        let e = Effect::ALL[i];
        let style = if i == sel { Style::new().fg(Color::Black).bg(Color::Cyan) } else { Style::new() };
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        let name = format!(" {}{}", if i == sel { "> " } else { "  " }, e.name());
        f.render_widget(Paragraph::new(Span::styled(format!("{name:<w$}", w = inner.width as usize), style)), rect);
        app.hits.push((rect, Hit::Effect(i)));
    }

    // Settings.
    let block = Block::bordered().title(format!(" Settings for \"{}\" ", app.lighting.effect.name()));
    let a = block.inner(settings_area);
    f.render_widget(block, settings_area);
    let c = app.lighting.effect.controls();
    let l = app.lighting;
    let unused = "(this effect does not use it)";
    let mut y = a.y;
    // Picking a color while Rainbow is on switches Rainbow off.
    y = color_controls(f, &mut app.hits, a, y, Target::Effect, l.color, c.color) + 1;

    let mut r = Row::new(a, y, &mut app.hits);
    r.text(f, "Rainbow    ", if c.color { LABEL } else { DIM });
    let check = if l.rainbow { "[x] On " } else { "[ ] Off" };
    let rainbow_note = if l.rainbow {
        "All colors cycle. Pick a color above to use one color."
    } else {
        "Click to cycle through all colors instead of one color."
    };
    r.button(f, check, if c.color { Style::new().fg(Color::White) } else { DIM }, c.color.then_some(Hit::Rainbow)).gap(2);
    r.text(f, if c.color { rainbow_note } else { unused }, DIM);
    y += 2;

    level_row(f, &mut app.hits, a, y, "Brightness", Level::Brightness, l.brightness, MAX_LEVEL, c.brightness, if c.brightness { "" } else { unused });
    level_row(f, &mut app.hits, a, y + 1, "Speed", Level::Speed, l.speed, MAX_LEVEL, c.speed, if c.speed { "" } else { unused });
    level_row(f, &mut app.hits, a, y + 2, "Direction", Level::Direction, l.direction, 3, c.direction, if c.direction { "Code 0-3; the result depends on the effect." } else { unused });
    y += 4;

    if y < a.bottom() {
        let mut r = Row::new(a, y, &mut app.hits);
        r.button(f, "  Apply to keyboard  ", BTN_MAIN, Some(Hit::Apply));
    }
}

/// Screen rectangle of each key, scaled to fit `area`.
fn key_rects(area: Rect) -> Vec<(&'static Key, Rect)> {
    let units = KEYS.iter().map(|k| (k.x4 + k.w4) as u16).max().unwrap_or(73);
    // Columns per quarter-key, at least 1.
    let scale = (area.width as f32 / units as f32).clamp(1.0, 1.6);
    let row_h: u16 = if area.height >= 6 * 2 + 1 { 2 } else { 1 };
    let total_w = (units as f32 * scale) as u16;
    let x0 = area.x + area.width.saturating_sub(total_w) / 2;
    KEYS.iter()
        .map(|k| {
            let x = x0 + (k.x4 as f32 * scale).round() as u16;
            let w = ((k.w4 as f32 * scale).round() as u16).saturating_sub(1).max(1);
            // Leave a blank line under the function row.
            let y = area.y + k.row as u16 * row_h + if k.row > 0 { 1 } else { 0 };
            (k, Rect::new(x, y, w, row_h))
        })
        .collect()
}

fn draw_key_colors(f: &mut Frame, app: &mut App, body: Rect) {
    let block = Block::bordered().title(" Per-key colors: pick a brush color, then click or drag over keys ");
    let a = block.inner(body);
    f.render_widget(block, body);
    let kb_h = (a.height.saturating_sub(5)).min(13);
    let kb_area = Rect::new(a.x, a.y, a.width, kb_h);
    for (k, rect) in key_rects(kb_area) {
        if rect.bottom() > kb_area.bottom() + 1 {
            continue;
        }
        let c = app.key_colors[k.index as usize];
        let style = if c == Rgb::default() {
            Style::new().bg(Color::Rgb(40, 40, 40)).fg(Color::Gray)
        } else {
            Style::new().bg(rgb(c)).fg(text_on(c))
        };
        let label: String = k.label.chars().take(rect.width as usize).collect();
        let mut lines = vec![Line::from(format!("{label:<w$}", w = rect.width as usize))];
        for _ in 1..rect.height {
            lines.push(Line::from(" ".repeat(rect.width as usize)));
        }
        f.render_widget(Paragraph::new(lines).style(style), rect);
        app.hits.push((rect, Hit::Key(k.index)));
    }
    let mut y = kb_area.bottom() + 1;
    y = color_controls(f, &mut app.hits, a, y, Target::Brush, app.brush, true);
    let mut r = Row::new(a, y, &mut app.hits);
    r.button(f, " Paint all keys ", BTN, Some(Hit::FillAll)).gap(1);
    r.button(f, " Turn all keys off ", BTN, Some(Hit::AllOff)).gap(3);
    let lit = KEYS.iter().filter(|k| app.key_colors[k.index as usize] != Rgb::default()).count();
    r.text(f, &format!("{lit} of {} keys lit", KEYS.len()), DIM);
    level_row(f, &mut app.hits, a, y + 1, "Brightness", Level::KeyBrightness, app.key_brightness, MAX_LEVEL, true, "");
    if y + 2 < a.bottom() {
        let mut r = Row::new(a, y + 2, &mut app.hits);
        r.button(f, "  Apply to keyboard  ", BTN_MAIN, Some(Hit::Apply)).gap(2);
        r.text(f, "Starts with all keys off: the keyboard cannot report its colors.", DIM);
    }
}

fn draw_device(f: &mut Frame, app: &mut App, body: Rect) {
    let block = Block::bordered().title(" Device ");
    let a = block.inner(body);
    f.render_widget(block, body);
    let ports = list_interfaces();
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Connection   ", LABEL),
            Span::raw(app.link.map(|l| l.label()).unwrap_or("Not connected")),
        ]),
        Line::from(vec![
            Span::styled("Battery      ", LABEL),
            Span::raw(match (app.link, app.battery) {
                (Some(Link::Dongle), Some(b)) => format!("{b}%"),
                (Some(Link::Dongle), None) => "Unknown (keyboard did not answer)".into(),
                (Some(Link::Wired), _) => "Not reported over the USB cable (charging)".into(),
                (None, _) => "-".into(),
            }),
        ]),
        Line::from(""),
        Line::styled("USB interfaces found", LABEL),
    ];
    for p in &ports {
        lines.push(Line::from(format!(
            "  {:<18} port {:<2}  usage {:04X}:{:04X}  {}",
            p.link.label(),
            p.info.interface,
            p.info.usage_page,
            p.info.usage,
            p.role.label()
        )));
    }
    if ports.is_empty() {
        lines.push(Line::styled("  None. Connect the USB cable or plug in the 2.4 GHz receiver.", DIM));
    }
    let n = lines.len() as u16;
    f.render_widget(Paragraph::new(lines), Rect::new(a.x, a.y, a.width, n.min(a.height)));
    if a.y + n + 1 < a.bottom() {
        let mut r = Row::new(a, a.y + n + 1, &mut app.hits);
        r.button(f, "  Check connection again  ", BTN, Some(Hit::Recheck));
    }
}

// ---------------------------------------------------------------- loop

pub fn run() -> Result<()> {
    let mut terminal = ratatui::init();
    execute!(stdout(), EnableMouseCapture)?;
    let result = event_loop(&mut terminal);
    let _ = execute!(stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut DefaultTerminal) -> Result<()> {
    let mut app = App::new();
    let mut dragging = false;
    while !app.quit {
        terminal.draw(|f| draw(f, &mut app))?;
        if app.pending_apply {
            app.pending_apply = false;
            app.apply_now();
            continue;
        }
        if app.last_scan.elapsed() > Duration::from_secs(60) {
            app.scan(true);
        }
        if !event::poll(Duration::from_millis(1000))? {
            // Cheap connection check without talking to the keyboard.
            if app.last_scan.elapsed() > Duration::from_secs(3) {
                let battery = app.battery;
                app.scan(false);
                if app.battery.is_none() {
                    app.battery = battery;
                }
            }
            continue;
        }
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => app.on_key(k.code, k.modifiers),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    dragging = true;
                    if let Some(h) = app.hit_at(m.column, m.row) {
                        app.on_hit(h);
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) if dragging => {
                    if let Some(h @ Hit::Key(_)) = app.hit_at(m.column, m.row) {
                        app.on_hit(h);
                    }
                }
                MouseEventKind::Up(_) => dragging = false,
                MouseEventKind::ScrollUp => app.on_scroll(m.column, m.row, true),
                MouseEventKind::ScrollDown => app.on_scroll(m.column, m.row, false),
                _ => {}
            },
            _ => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;

    fn render(app: &mut App, w: u16, h: u16) -> String {
        let mut t = Terminal::new(TestBackend::new(w, h)).unwrap();
        t.draw(|f| draw(f, app)).unwrap();
        let buf = t.backend().buffer().clone();
        let mut out = String::new();
        for y in 0..h {
            for x in 0..w {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push(char::from(10));
        }
        out
    }

    #[test]
    fn pages_render_and_click() {
        let mut app = App::blank();
        app.link = Some(Link::Dongle);
        app.battery = Some(92);
        let s = render(&mut app, 100, 30);
        println!("{s}");
        assert!(s.contains("Battery: 92%"));
        assert!(s.contains("Apply to keyboard"));

        // Click the "breath" effect, then the Blue swatch.
        let (r, _) = *app.hits.iter().find(|(_, h)| *h == Hit::Effect(7)).unwrap();
        app.on_hit(app.hit_at(r.x, r.y).unwrap());
        assert_eq!(app.lighting.effect, Effect::Breath);
        let (r, _) = *app.hits.iter().find(|(_, h)| *h == Hit::Palette(Target::Effect, 5)).unwrap();
        app.on_hit(app.hit_at(r.x + 1, r.y).unwrap());
        assert_eq!(app.lighting.color, Rgb::new(0, 0, 0xFF));

        app.tab = Tab::KeyColors;
        let s = render(&mut app, 100, 30);
        println!("{s}");
        let (r, _) = *app.hits.iter().find(|(_, h)| *h == Hit::Key(1)).unwrap();
        app.on_hit(app.hit_at(r.x, r.y).unwrap());
        assert_eq!(app.key_colors[1], app.brush);

        app.tab = Tab::Device;
        println!("{}", render(&mut app, 100, 30));
        let small = render(&mut app, 60, 20);
        assert!(small.contains("too small"));
    }
}
