//! Screen layout and the six pages.

use m87_hid::list_interfaces;
use m87_proto::keymap::{modifier, KeyAction, Layer, Mouse, MEDIA};
use m87_proto::layout::KEYS;
use m87_proto::lighting::{Effect, Rgb, MAX_LEVEL};
use m87_proto::settings::SLEEP_CHOICES;
use m87_proto::Link;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Paragraph, Wrap};
use ratatui::Frame;

use super::widgets::*;
use super::{App, Hit, Level, Tab, Target, Tone};

const MIN_W: u16 = 80;
const MIN_H: u16 = 24;

pub fn draw(f: &mut Frame, app: &mut App) {
    app.hits.clear();
    let area = f.area();
    if area.width < MIN_W || area.height < MIN_H {
        let msg = format!(
            "The window is too small ({}x{}). Make it at least {MIN_W}x{MIN_H}. Press Q to quit.",
            area.width, area.height
        );
        f.render_widget(Paragraph::new(msg).wrap(Wrap { trim: true }), area);
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
        Tab::Lighting => lighting(f, app, body),
        Tab::KeyColors => key_colors(f, app, body),
        Tab::Keys => key_mapping(f, app, body),
        Tab::Macros => macros(f, app, body),
        Tab::Settings => settings(f, app, body),
        Tab::Device => device(f, app, body),
    }

    let tone = match app.status.0 {
        Tone::Info => Style::new().fg(Color::White),
        Tone::Ok => Style::new().fg(Color::LightGreen),
        Tone::Err => Style::new().fg(Color::LightRed),
    };
    f.render_widget(Paragraph::new(Span::styled(format!(" {}", app.status.1), tone)), status);
    let hint = if app.macro_editing {
        " Typing a macro: Enter = save and send, Esc = cancel, Left/Right/Home/End = move"
    } else {
        " 1-6 page  Up/Down select  Left/Right brightness  Enter apply  Q quit  Mouse: click, drag, scroll"
    };
    f.render_widget(Paragraph::new(Span::styled(hint, DIM)), help);
}

fn apply_button(f: &mut Frame, hits: &mut Vec<(Rect, Hit)>, area: Rect, y: u16, note: &str) {
    if y < area.bottom() {
        let mut r = Row::new(area, y, hits);
        r.button(f, "  Apply to keyboard  ", BTN_MAIN, Some(Hit::Apply)).gap(2);
        r.text(f, note, Style::new().fg(Color::Yellow));
    }
}

// ------------------------------------------------------------ 1 Lighting

fn lighting(f: &mut Frame, app: &mut App, body: Rect) {
    let [list_area, settings_area] = Layout::horizontal([Constraint::Length(22), Constraint::Min(40)]).areas(body);

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

    let block = Block::bordered().title(format!(" Settings for \"{}\" ", app.lighting.effect.name()));
    let a = block.inner(settings_area);
    f.render_widget(block, settings_area);
    let c = app.lighting.effect.controls();
    let l = app.lighting;
    let unused = "(this effect does not use it)";
    // Picking a color while Rainbow is on switches Rainbow off.
    let mut y = color_controls(f, &mut app.hits, a, a.y, Target::Effect, l.color, c.color) + 1;

    let mut r = Row::new(a, y, &mut app.hits);
    r.text(f, "Rainbow      ", if c.color { LABEL } else { DIM });
    let check = if l.rainbow { "[x] On " } else { "[ ] Off" };
    r.button(f, check, if c.color { Style::new().fg(Color::White) } else { DIM }, c.color.then_some(Hit::Rainbow)).gap(2);
    let note = if !c.color {
        unused
    } else if l.rainbow {
        "All colors cycle. Pick a color above to use one color."
    } else {
        "Click to cycle through all colors instead of one color."
    };
    r.text(f, note, DIM);
    y += 2;

    level_row(f, &mut app.hits, a, y, "Brightness", Level::Brightness, l.brightness, MAX_LEVEL, c.brightness, if c.brightness { "" } else { unused });
    level_row(f, &mut app.hits, a, y + 1, "Speed", Level::Speed, l.speed, MAX_LEVEL, c.speed, if c.speed { "" } else { unused });
    level_row(f, &mut app.hits, a, y + 2, "Direction", Level::Direction, l.direction, 3, c.direction, if c.direction { "Code 0-3; the result depends on the effect." } else { unused });
    apply_button(f, &mut app.hits, a, y + 4, "");
}

// ------------------------------------------------------------ 2 Key colors

fn key_colors(f: &mut Frame, app: &mut App, body: Rect) {
    let block = Block::bordered().title(" Key colors: pick a brush color, then click or drag over keys ");
    let a = block.inner(body);
    f.render_widget(block, body);
    let kb_h = a.height.saturating_sub(6).min(13);
    let kb_area = Rect::new(a.x, a.y, a.width, kb_h);
    for (k, rect) in key_rects(kb_area) {
        let c = app.key_colors[k.index as usize];
        let style = if c == Rgb::default() {
            Style::new().bg(Color::Rgb(40, 40, 40)).fg(Color::Gray)
        } else {
            Style::new().bg(rgb(c)).fg(text_on(c))
        };
        key_cap(f, rect, k.label, style);
        app.hits.push((rect, Hit::PaintKey(k.index)));
    }
    let y = color_controls(f, &mut app.hits, a, kb_area.bottom() + 1, Target::Brush, app.brush, true);
    let mut r = Row::new(a, y, &mut app.hits);
    r.button(f, " Paint all keys ", BTN, Some(Hit::FillAll)).gap(1);
    r.button(f, " Turn all keys off ", BTN, Some(Hit::AllOff)).gap(3);
    let lit = KEYS.iter().filter(|k| app.key_colors[k.index as usize] != Rgb::default()).count();
    r.text(f, &format!("{lit} of {} keys lit", KEYS.len()), DIM);
    level_row(f, &mut app.hits, a, y + 1, "Brightness", Level::KeyBrightness, app.key_brightness, MAX_LEVEL, true, "");
    apply_button(f, &mut app.hits, a, y + 2, "");
    if y + 3 < a.bottom() {
        f.render_widget(
            Paragraph::new(Span::styled("This page starts with all keys off because the keyboard cannot report its colors.", DIM)),
            Rect::new(a.x, y + 3, a.width, 1),
        );
    }
}

// ------------------------------------------------------------ 3 Key mapping

fn action_label(a: KeyAction) -> String {
    match a {
        KeyAction::Default => "normal function".into(),
        KeyAction::Disabled => "nothing (disabled)".into(),
        other => other.to_text(),
    }
}

fn key_mapping(f: &mut Frame, app: &mut App, body: Rect) {
    let block = Block::bordered().title(" Key mapping: click a key, then choose what it does ");
    let a = block.inner(body);
    f.render_widget(block, body);

    // Layer switch.
    let mut r = Row::new(a, a.y, &mut app.hits);
    r.text(f, "Layer   ", LABEL);
    for (l, name) in [(Layer::Base, " Normal "), (Layer::Fn, " While Fn is held ")] {
        r.button(f, name, if app.layer == l { BTN_ON } else { BTN }, Some(Hit::Layer(l))).gap(1);
    }
    r.gap(2).text(f, "Changed keys are yellow.", DIM);

    // Keyboard, one line per row so the action list fits below.
    let kb_area = Rect::new(a.x, a.y + 2, a.width, 7);
    for (k, rect) in key_rects(kb_area) {
        let action = app.state.key(app.layer, k.id);
        let selected = app.map_key.map(|m| m.index) == Some(k.index);
        let style = if selected {
            Style::new().fg(Color::Black).bg(Color::Cyan).add_modifier(Modifier::BOLD)
        } else if app.choosing_key {
            Style::new().fg(Color::White).bg(Color::Blue)
        } else if action != KeyAction::Default {
            Style::new().fg(Color::Black).bg(Color::Yellow)
        } else {
            Style::new().fg(Color::Gray).bg(Color::Rgb(40, 40, 40))
        };
        key_cap(f, rect, k.label, style);
        app.hits.push((rect, Hit::MapKey(k.index)));
    }

    let mut y = kb_area.bottom() + 1;
    let Some(key) = app.map_key else {
        f.render_widget(Paragraph::new(Span::styled("Click a key above to change it.", LABEL)), Rect::new(a.x, y, a.width, 1));
        reset_and_apply(f, app, a, y + 2);
        return;
    };
    let current = app.state.key(app.layer, key.id);
    let layer_text = match app.layer {
        Layer::Base => "normal layer",
        Layer::Fn => "while Fn is held",
    };
    let line = Line::from(vec![
        Span::styled("Selected ", LABEL),
        Span::styled(format!("{} ", key.id), Style::new().add_modifier(Modifier::BOLD)),
        Span::styled(format!("({layer_text})  now does: "), LABEL),
        Span::styled(action_label(current), Style::new().fg(Color::Yellow)),
    ]);
    f.render_widget(Paragraph::new(line), Rect::new(a.x, y, a.width, 1));
    y += 1;
    if key.id == "Fn" {
        f.render_widget(Paragraph::new(Span::styled("The Fn key cannot be changed.", DIM)), Rect::new(a.x, y, a.width, 1));
        reset_and_apply(f, app, a, y + 2);
        return;
    }

    let on = |cond: bool| if cond { BTN_ON } else { BTN };
    let mut r = Row::new(a, y, &mut app.hits);
    r.text(f, "Basic    ", LABEL).indent(9);
    r.button(f, " Normal function ", on(current == KeyAction::Default), Some(Hit::SetAction(KeyAction::Default))).gap(1);
    r.button(f, " Do nothing ", on(current == KeyAction::Disabled), Some(Hit::SetAction(KeyAction::Disabled)));

    let mut r = Row::new(a, r.y + 1, &mut app.hits);
    r.text(f, "Key      ", LABEL).indent(9);
    let (mods, usage) = match current {
        KeyAction::Key { modifiers, usage } => (modifiers, usage),
        _ => (0, 0),
    };
    let target = if usage == 0 {
        "Choose key".to_string()
    } else {
        format!("Sends {}", KeyAction::Key { modifiers: mods, usage }.to_text().trim_start_matches("key:"))
    };
    r.button(f, &format!(" {target} "), on(app.choosing_key || usage != 0), Some(Hit::ChooseKey)).gap(2);
    r.text(f, "with ", LABEL);
    for (name, bit) in [("Ctrl", modifier::LCTRL), ("Shift", modifier::LSHIFT), ("Alt", modifier::LALT), ("Win", modifier::LWIN)] {
        r.button(f, &format!(" {name} "), on(mods & bit != 0), Some(Hit::Modifier(bit))).gap(1);
    }

    let mut r = Row::new(a, r.y + 1, &mut app.hits);
    r.text(f, "Mouse    ", LABEL).indent(9);
    for m in Mouse::ALL {
        let act = KeyAction::Mouse(m);
        r.button(f, &format!(" {} ", m.name()), on(current == act), Some(Hit::SetAction(act))).gap(1);
    }

    let mut r = Row::new(a, r.y + 1, &mut app.hits);
    r.text(f, "Media    ", LABEL).indent(9);
    for (name, u) in MEDIA {
        let act = KeyAction::Media(u);
        r.button(f, &format!(" {name} "), on(current == act), Some(Hit::SetAction(act))).gap(1);
    }

    let mut r = Row::new(a, r.y + 1, &mut app.hits);
    r.text(f, "Macro    ", LABEL).indent(9);
    if app.state.macros.is_empty() {
        r.text(f, "No macros yet. Create one on the Macros page.", DIM);
    }
    let numbers: Vec<u8> = app.state.macros.keys().copied().collect();
    for n in numbers {
        let act = KeyAction::Macro { slot: n - 1, mode: 0, count: 1 };
        r.button(f, &format!(" Macro {n} "), on(current == act), Some(Hit::SetAction(act))).gap(1);
    }
    let y = r.y + 2;
    reset_and_apply(f, app, a, y);
}

fn reset_and_apply(f: &mut Frame, app: &mut App, a: Rect, y: u16) {
    if y >= a.bottom() {
        return;
    }
    let unsent = [Layer::Base, Layer::Fn].iter().any(|l| app.state.entries(*l) != app.saved.entries(*l));
    let mut r = Row::new(a, y, &mut app.hits);
    r.button(f, "  Apply to keyboard  ", BTN_MAIN, Some(Hit::Apply)).gap(2);
    r.button(f, " Reset all keys to normal ", BTN_WARN, Some(Hit::ResetAllKeys)).gap(2);
    if unsent {
        r.text(f, "Changes not sent yet.", Style::new().fg(Color::Yellow));
    }
}

// ------------------------------------------------------------ 4 Macros

fn macros(f: &mut Frame, app: &mut App, body: Rect) {
    let [list_area, edit_area] = Layout::horizontal([Constraint::Length(30), Constraint::Min(40)]).areas(body);

    let block = Block::bordered().title(" Macros ");
    let inner = block.inner(list_area);
    f.render_widget(block, list_area);
    let rows = inner.height as usize;
    let start = (app.macro_sel as usize).saturating_sub(rows.saturating_sub(1)).max(1);
    for (row, n) in (start..=100).take(rows).enumerate() {
        let n = n as u8;
        let text = app.state.macros.get(&n).map(|m| m.to_text()).unwrap_or_else(|| "(empty)".into());
        let sel = n == app.macro_sel;
        let style = if sel {
            Style::new().fg(Color::Black).bg(Color::Cyan)
        } else if app.state.macros.contains_key(&n) {
            Style::new()
        } else {
            DIM
        };
        let line = format!(" {n:>3}  {text}");
        let w = inner.width as usize;
        let line: String = line.chars().take(w).collect();
        let rect = Rect::new(inner.x, inner.y + row as u16, inner.width, 1);
        f.render_widget(Paragraph::new(Span::styled(format!("{line:<w$}"), style)), rect);
        app.hits.push((rect, Hit::MacroSelect(n)));
    }

    let block = Block::bordered().title(format!(" Macro {} ", app.macro_sel));
    let a = block.inner(edit_area);
    f.render_widget(block, edit_area);
    f.render_widget(Paragraph::new(Span::styled("Steps (comma-separated):", LABEL)), Rect::new(a.x, a.y, a.width, 1));

    // Text field.
    let field = Rect::new(a.x, a.y + 1, a.width, 1);
    let (view, cursor) = app.macro_input.view(field.width as usize);
    let style = if app.macro_editing { Style::new().fg(Color::White).bg(Color::Rgb(30, 30, 90)) } else { Style::new().fg(Color::White).bg(Color::Rgb(40, 40, 40)) };
    f.render_widget(Paragraph::new(Span::styled(format!("{view:<w$}", w = field.width as usize), style)), field);
    app.hits.push((field, Hit::MacroInput));
    if app.macro_editing {
        f.set_cursor_position(Position::new(field.x + cursor as u16, field.y));
    } else if app.macro_input.text().is_empty() {
        f.render_widget(Paragraph::new(Span::styled("Click here and type, e.g. tap:H, tap:I, wait:100, tap:Enter", DIM.bg(Color::Rgb(40, 40, 40)))), field);
    }

    let help = [
        "tap:A              press and release a key",
        "down:LShift        hold a key down",
        "up:LShift          release a held key",
        "click:left         mouse click (left, right, middle)",
        "mouse-down:left    hold a mouse button (mouse-up:left releases)",
        "wait:200           pause 200 ms before the next step",
        "Every step is followed by a 20 ms pause.",
    ];
    for (i, h) in help.iter().enumerate() {
        let y = a.y + 3 + i as u16;
        if y < a.bottom() {
            f.render_widget(Paragraph::new(Span::styled(*h, DIM)), Rect::new(a.x, y, a.width, 1));
        }
    }
    let y = a.y + 4 + help.len() as u16;
    if y < a.bottom() {
        let mut r = Row::new(a, y, &mut app.hits);
        r.button(f, "  Save and send all macros  ", BTN_MAIN, Some(Hit::MacroSave)).gap(2);
        r.button(f, " Delete this macro ", BTN_WARN, Some(Hit::MacroDelete));
    }
    if y + 2 < a.bottom() {
        f.render_widget(
            Paragraph::new(Span::styled("To play a macro, assign it to a key on the Key mapping page.", DIM)),
            Rect::new(a.x, y + 2, a.width, 1),
        );
    }
}

// ------------------------------------------------------------ 5 Settings

fn settings(f: &mut Frame, app: &mut App, body: Rect) {
    let block = Block::bordered().title(" Keyboard settings ");
    let a = block.inner(body);
    f.render_widget(block, body);
    let s = app.state.settings;
    let mut y = a.y;
    for (i, (name, on, note)) in [
        ("Block Windows key", s.disable_win, "Stops the Start menu opening by accident, e.g. in games."),
        ("Block Alt+Tab", s.disable_alt_tab, "Stops switching windows by accident."),
        ("Block Alt+F4", s.disable_alt_f4, "Stops closing windows by accident."),
    ]
    .into_iter()
    .enumerate()
    {
        let mut r = Row::new(a, y, &mut app.hits);
        r.text(f, &format!("{name:<20}"), LABEL);
        r.button(f, if on { "[x] On " } else { "[ ] Off" }, Style::new().fg(Color::White), Some(Hit::Toggle(i as u8))).gap(2);
        r.text(f, note, DIM);
        y += 1;
    }
    y += 1;
    let mut r = Row::new(a, y, &mut app.hits);
    r.text(f, &format!("{:<20}", "Sleep after"), LABEL);
    for (name, v) in SLEEP_CHOICES {
        let label = match name {
            "never" => " Never ",
            "1min" => " 1 minute ",
            "5min" => " 5 minutes ",
            _ => " 30 minutes ",
        };
        r.button(f, label, if s.sleep == v { BTN_ON } else { BTN }, Some(Hit::Sleep(v))).gap(1);
    }
    r.gap(1).text(f, "Saves battery.", DIM);
    y += 2;
    let mut r = Row::new(a, y, &mut app.hits);
    r.text(f, &format!("{:<20}", "Key response"), LABEL);
    r.button(f, " - ", BTN, Some(Hit::Step(Level::Response, -1))).gap(1);
    r.button(f, &format!("Level {}", s.response), Style::new().fg(Color::White), Some(Hit::Step(Level::Response, 0))).gap(1);
    r.button(f, " + ", BTN, Some(Hit::Step(Level::Response, 1))).gap(2);
    r.text(f, "1 = fastest. Raise it if one press types twice.", DIM);
    y += 2;
    apply_button(f, &mut app.hits, a, y, if s != app.saved.settings { "Changes not sent yet." } else { "" });
    y += 2;
    for line in [
        "The keyboard cannot report these settings. This page shows what was last sent from this PC.",
        "Sleep and response values follow the original app; their exact timing is not yet confirmed.",
    ] {
        if y < a.bottom() {
            f.render_widget(Paragraph::new(Span::styled(line, DIM)), Rect::new(a.x, y, a.width, 1));
        }
        y += 1;
    }
}

// ------------------------------------------------------------ 6 Device

fn device(f: &mut Frame, app: &mut App, body: Rect) {
    let block = Block::bordered().title(" Device ");
    let a = block.inner(body);
    f.render_widget(block, body);
    let ports = list_interfaces();
    let mut lines = vec![
        Line::from(vec![Span::styled("Connection   ", LABEL), Span::raw(app.link.map(|l| l.label()).unwrap_or("Not connected"))]),
        Line::from(vec![
            Span::styled("Battery      ", LABEL),
            Span::raw(match (app.link, app.battery) {
                (Some(Link::Dongle), Some(b)) => format!("{b}%"),
                (Some(Link::Dongle), None) => "Unknown (keyboard did not answer)".into(),
                (Some(Link::Wired), _) => "Not reported over the USB cable (charging)".into(),
                (None, _) => "-".into(),
            }),
        ]),
        Line::from(vec![Span::styled("Saved data   ", LABEL), Span::raw(crate::state::path().display().to_string())]),
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
