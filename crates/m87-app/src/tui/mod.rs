//! Full-screen terminal interface with mouse support.
//!
//! Every clickable element registers its screen rectangle in `hits` while it
//! is drawn; mouse events are matched against that list.

mod input;
mod pages;
mod widgets;

use std::io::stdout;
use std::time::{Duration, Instant};

use anyhow::Result;
use m87_hid::{list_interfaces, Keyboard, Role};
use m87_proto::keymap::{KeyAction, Layer};
use m87_proto::layout::{self, KEYS, SLOTS};
use m87_proto::lighting::{Effect, Lighting, Rgb, MAX_LEVEL};
use m87_proto::macros::Macro;
use m87_proto::plan::{self, Packet};
use m87_proto::Link;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton,
    MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Position, Rect};
use ratatui::{DefaultTerminal, Frame};

use crate::state::{self, LightMode, State};
use input::Input;

pub const PALETTE: [(&str, Rgb); 10] = [
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

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Lighting,
    KeyColors,
    Keys,
    Macros,
    Settings,
    Profiles,
}

impl Tab {
    pub const ALL: [Tab; 6] = [Tab::Lighting, Tab::KeyColors, Tab::Keys, Tab::Macros, Tab::Settings, Tab::Profiles];
    pub fn title(self) -> &'static str {
        match self {
            Tab::Lighting => "1 Lighting",
            Tab::KeyColors => "2 Key colors",
            Tab::Keys => "3 Key mapping",
            Tab::Macros => "4 Macros",
            Tab::Settings => "5 Settings",
            Tab::Profiles => "6 Profiles",
        }
    }
    /// Pages that end with an "Apply to keyboard" button.
    fn sends(self) -> bool {
        !matches!(self, Tab::Profiles)
    }
}

/// Which color the palette and RGB controls edit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Effect,
    Brush,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Level {
    Brightness,
    Speed,
    Direction,
    KeyBrightness,
    Response,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Tab(Tab),
    Effect(usize),
    Palette(Target, usize),
    Channel(Target, usize, i16),
    Rainbow,
    Step(Level, i8),
    Apply,
    PaintKey(u8),
    FillAll,
    AllOff,
    Recheck,
    Quit,
    // Key mapping page
    Layer(Layer),
    MapKey(u8),
    SetAction(KeyAction),
    Modifier(u8),
    ChooseKey,
    ResetAllKeys,
    // Macros page
    MacroSelect(u8),
    MacroInput,
    MacroSave,
    MacroDelete,
    // Settings page
    Toggle(u8),
    Sleep(u8),
    // Profiles page
    ProfileSelect(usize),
    ProfileName,
    ProfileSave,
    ProfileLoad,
    ProfileDelete,
    OpenFolder,
    FactoryReset,
    /// Answer to the open confirmation question.
    Confirm(bool),
}

/// A question that must be answered before something is erased.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Confirm {
    DeleteProfile(String),
    FactoryReset,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Info,
    Ok,
    Err,
}

/// What the next "Apply" sends.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Pending {
    Page(Tab),
    ResetKeys,
    LoadProfile(String),
    FactoryReset,
}

pub struct App {
    pub tab: Tab,
    pub lighting: Lighting,
    pub key_colors: [Rgb; SLOTS],
    pub brush: Rgb,
    pub key_brightness: u8,
    pub effect_scroll: usize,
    pub status: (Tone, String),
    pub link: Option<Link>,
    pub battery: Option<u8>,
    last_scan: Instant,
    pending: Option<Pending>,
    pub hits: Vec<(Rect, Hit)>,
    pub quit: bool,
    /// Draft of everything saved on this PC (key map, macros, settings).
    pub state: State,
    /// Last state that was sent successfully (to show unsent changes).
    pub saved: State,
    pub layer: Layer,
    pub map_key: Option<&'static layout::Key>,
    pub choosing_key: bool,
    pub macro_sel: u8,
    pub macro_input: Input,
    pub macro_editing: bool,
    pub profiles: Vec<String>,
    pub profile_sel: Option<usize>,
    pub name_input: Input,
    pub name_editing: bool,
    pub confirm: Option<Confirm>,
}

impl App {
    fn new() -> Self {
        let mut app = App::blank();
        match State::load() {
            Ok(st) => {
                app.state = st.clone();
                app.saved = st;
            }
            Err(e) => app.status = (Tone::Err, format!("Saved settings could not be read: {e:#}")),
        }
        app.sync_from_state();
        app.profiles = state::list_profiles();
        app.scan(true);
        app
    }

    pub fn blank() -> Self {
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
            pending: None,
            hits: Vec::new(),
            quit: false,
            state: State::default(),
            saved: State::default(),
            layer: Layer::Base,
            map_key: None,
            choosing_key: false,
            macro_sel: 1,
            macro_input: Input::default(),
            macro_editing: false,
            profiles: Vec::new(),
            profile_sel: None,
            name_input: Input::default(),
            name_editing: false,
            confirm: None,
        }
    }

    /// Copy the lighting and key colors of `state` into the page drafts.
    fn sync_from_state(&mut self) {
        self.lighting = self.state.lighting;
        self.key_colors = self.state.color_table();
        self.key_brightness = self.state.key_brightness;
        self.load_macro_text();
    }

    pub fn selected_profile(&self) -> Option<&str> {
        self.profile_sel.and_then(|i| self.profiles.get(i)).map(|s| s.as_str())
    }

    fn refresh_profiles(&mut self, select: Option<&str>) {
        self.profiles = state::list_profiles();
        self.profile_sel = select.and_then(|n| self.profiles.iter().position(|p| p.eq_ignore_ascii_case(n)));
    }

    fn save_profile(&mut self) {
        let name = self.name_input.text();
        let name = match state::check_profile_name(&name) {
            Ok(n) => n.to_string(),
            Err(e) => {
                self.status = (Tone::Err, format!("Not saved: {e}."));
                return;
            }
        };
        self.name_editing = false;
        let existed = self.profiles.iter().any(|p| p.eq_ignore_ascii_case(&name));
        match self.saved.save_profile(&name) {
            Ok(_) => {
                self.refresh_profiles(Some(&name));
                let verb = if existed { "replaced" } else { "saved" };
                self.status = (Tone::Ok, format!("Profile \"{name}\" {verb}."));
            }
            Err(e) => self.status = (Tone::Err, format!("Not saved: {e:#}.")),
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

    /// Packets for the pending operation, a success message, and the new
    /// complete state when the operation replaces everything.
    fn packets(&self, what: &Pending, link: Link) -> std::result::Result<(Vec<Packet>, String, Option<State>), String> {
        let (p, msg) = match what {
            Pending::LoadProfile(name) => {
                let st = State::load_profile(name).map_err(|e| format!("{e:#}."))?;
                let p = st.full_plan(link)?;
                return Ok((p, format!("Profile \"{name}\" is now on the keyboard."), Some(st)));
            }
            Pending::FactoryReset => {
                return Ok((plan::factory_reset(link), "The keyboard is back to its factory settings.".into(), Some(State::default())));
            }
            Pending::ResetKeys => (plan::reset_key_maps(link), "Every key does its normal function again.".into()),
            Pending::Page(Tab::KeyColors) => {
                let colors: Vec<(u8, Rgb)> = KEYS.iter().map(|k| (k.index, self.key_colors[k.index as usize])).collect();
                (plan::set_key_colors(link, self.key_brightness, &colors), "Per-key colors sent.".into())
            }
            Pending::Page(Tab::Keys) => {
                let mut p = Vec::new();
                for layer in [Layer::Base, Layer::Fn] {
                    if self.state.entries(layer) != self.saved.entries(layer) {
                        p.extend(plan::set_key_map(link, layer, &self.state.entries(layer)));
                    }
                }
                if p.is_empty() {
                    return Err("There are no key changes to send.".into());
                }
                (p, "Key mapping sent.".into())
            }
            Pending::Page(Tab::Macros) => {
                let buf = self.state.macro_storage()?;
                (plan::set_macros(link, &buf), "All macros sent.".into())
            }
            Pending::Page(Tab::Settings) => (plan::set_settings(link, &self.state.settings), "Settings sent.".into()),
            Pending::Page(Tab::Lighting) => (
                plan::set_effect(link, &self.lighting),
                format!("Effect \"{}\" sent.", self.lighting.effect.name()),
            ),
            Pending::Page(Tab::Profiles) => return Err("There is nothing to send on this page.".into()),
        };
        Ok((p, msg, None))
    }

    fn apply_now(&mut self, what: Pending) {
        let kb = match Keyboard::open() {
            Ok(kb) => kb,
            Err(e) => {
                self.status = (Tone::Err, format!("Not applied: {e}."));
                return;
            }
        };
        let link = kb.link();
        let (packets, msg, whole) = match self.packets(&what, link) {
            Ok(x) => x,
            Err(e) => {
                self.status = (Tone::Err, format!("Not applied: {e}"));
                return;
            }
        };
        if let Err(e) = kb.run(&packets) {
            self.status = (Tone::Err, format!("Not applied: {e}."));
            return;
        }
        self.link = Some(link);
        if let Some(st) = whole {
            self.state = st.clone();
            self.saved = st;
            self.sync_from_state();
        } else {
            // Only the part that was sent becomes "saved".
            match what {
                Pending::ResetKeys => {
                    self.state.clear_keys();
                    self.saved.keys.clear();
                }
                Pending::Page(Tab::Keys) => self.saved.keys = self.state.keys.clone(),
                Pending::Page(Tab::Macros) => self.saved.macros = self.state.macros.clone(),
                Pending::Page(Tab::Settings) => self.saved.settings = self.state.settings,
                Pending::Page(Tab::Lighting) => {
                    for st in [&mut self.state, &mut self.saved] {
                        st.light_mode = LightMode::Effect;
                        st.lighting = self.lighting;
                    }
                }
                Pending::Page(Tab::KeyColors) => {
                    for st in [&mut self.state, &mut self.saved] {
                        st.light_mode = LightMode::KeyColors;
                        st.set_color_table(&self.key_colors);
                        st.key_brightness = self.key_brightness;
                    }
                }
                _ => {}
            }
        }
        let note = match self.saved.save() {
            Ok(()) => String::new(),
            Err(e) => format!(" (Could not save on this PC: {e:#})"),
        };
        self.status = (Tone::Ok, format!("Applied over the {}. {msg}{note}", link.label()));
    }

    fn step(&mut self, level: Level, delta: i8) {
        let (v, min, max) = match level {
            Level::Brightness => (&mut self.lighting.brightness, 0, MAX_LEVEL),
            Level::Speed => (&mut self.lighting.speed, 0, MAX_LEVEL),
            Level::Direction => (&mut self.lighting.direction, 0, 3),
            Level::KeyBrightness => (&mut self.key_brightness, 0, MAX_LEVEL),
            Level::Response => (&mut self.state.settings.response, 1, 5),
        };
        *v = (*v as i16 + delta as i16).clamp(min, max as i16) as u8;
    }

    fn select_effect(&mut self, i: usize) {
        if let Some(e) = Effect::ALL.get(i) {
            self.lighting.effect = *e;
        }
    }

    pub fn current_action(&self) -> Option<KeyAction> {
        self.map_key.map(|k| self.state.key(self.layer, k.id))
    }

    fn set_current_action(&mut self, a: KeyAction) {
        if let Some(k) = self.map_key {
            self.state.set_key(self.layer, k.id, a);
            let layer = match self.layer {
                Layer::Base => "normal layer",
                Layer::Fn => "while Fn is held",
            };
            self.status = (Tone::Info, format!("{} ({layer}) will do {}. Click Apply to send.", k.id, a.to_text()));
        }
    }

    fn load_macro_text(&mut self) {
        let text = self.state.macros.get(&self.macro_sel).map(|m| m.to_text()).unwrap_or_default();
        self.macro_input = Input::new(text);
    }

    fn save_macro(&mut self) {
        let text = self.macro_input.text().trim().to_string();
        if text.is_empty() {
            self.status = (Tone::Err, "Type the steps first, or click Delete to remove this macro.".into());
            return;
        }
        match Macro::parse(&text) {
            Ok(m) => {
                self.state.macros.insert(self.macro_sel, m);
                if let Err(e) = self.state.macro_storage() {
                    self.status = (Tone::Err, format!("Not applied: {e}"));
                    return;
                }
                self.macro_editing = false;
                self.load_macro_text();
                self.status = (Tone::Info, "Sending all macros...".into());
                self.pending = Some(Pending::Page(Tab::Macros));
            }
            Err(e) => self.status = (Tone::Err, format!("Macro {} not saved: {e}.", self.macro_sel)),
        }
    }

    fn on_hit(&mut self, hit: Hit) {
        // Clicking anywhere else ends text editing.
        if !matches!(hit, Hit::MacroInput | Hit::MacroSave) {
            self.macro_editing = false;
        }
        if !matches!(hit, Hit::ProfileName | Hit::ProfileSave) {
            self.name_editing = false;
        }
        if !matches!(hit, Hit::Confirm(_)) {
            self.confirm = None;
        }
        match hit {
            Hit::Tab(t) => {
                self.tab = t;
                self.choosing_key = false;
                if t == Tab::Profiles {
                    let sel = self.selected_profile().map(String::from);
                    self.refresh_profiles(sel.as_deref());
                }
            }
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
                self.pending = Some(Pending::Page(self.tab));
            }
            Hit::PaintKey(i) => self.key_colors[i as usize] = self.brush,
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
            Hit::Layer(l) => self.layer = l,
            Hit::MapKey(i) => {
                let Some(key) = KEYS.iter().find(|k| k.index == i) else { return };
                if self.choosing_key {
                    self.choosing_key = false;
                    if key.id == "Fn" {
                        self.status = (Tone::Err, "Fn cannot be sent as a key. Choose another key.".into());
                        return;
                    }
                    let modifiers = match self.current_action() {
                        Some(KeyAction::Key { modifiers, .. }) => modifiers,
                        _ => 0,
                    };
                    self.set_current_action(KeyAction::Key { modifiers, usage: key.usage });
                } else {
                    self.map_key = Some(key);
                    self.status = if key.id == "Fn" {
                        (Tone::Err, "The Fn key cannot be changed.".into())
                    } else {
                        (Tone::Info, format!("Selected {}. Choose what it should do.", key.id))
                    };
                }
            }
            Hit::SetAction(a) => {
                self.choosing_key = false;
                self.set_current_action(a);
            }
            Hit::Modifier(bit) => {
                let (modifiers, usage) = match self.current_action() {
                    Some(KeyAction::Key { modifiers, usage }) if usage != 0 && !(0xE0..=0xE7).contains(&usage) => (modifiers, usage),
                    _ => {
                        self.status = (Tone::Err, "First click \"Choose key\" and pick the key to send, then add Ctrl/Shift/Alt/Win.".into());
                        return;
                    }
                };
                self.set_current_action(KeyAction::Key { modifiers: modifiers ^ bit, usage });
            }
            Hit::ChooseKey => {
                if self.map_key.is_none() {
                    self.status = (Tone::Err, "First click the key you want to change.".into());
                } else {
                    self.choosing_key = true;
                    self.status = (Tone::Info, "Now click the key it should send.".into());
                }
            }
            Hit::ResetAllKeys => {
                self.status = (Tone::Info, "Resetting all keys...".into());
                self.pending = Some(Pending::ResetKeys);
            }
            Hit::MacroSelect(n) => {
                self.macro_sel = n;
                self.load_macro_text();
            }
            Hit::MacroInput => self.macro_editing = true,
            Hit::MacroSave => self.save_macro(),
            Hit::MacroDelete => {
                if self.state.macros.remove(&self.macro_sel).is_some() {
                    self.load_macro_text();
                    self.status = (Tone::Info, "Sending the remaining macros...".into());
                    self.pending = Some(Pending::Page(Tab::Macros));
                } else {
                    self.status = (Tone::Err, format!("Macro {} is already empty.", self.macro_sel));
                }
            }
            Hit::Toggle(i) => {
                let s = &mut self.state.settings;
                match i {
                    0 => s.disable_win = !s.disable_win,
                    1 => s.disable_alt_tab = !s.disable_alt_tab,
                    _ => s.disable_alt_f4 = !s.disable_alt_f4,
                }
            }
            Hit::Sleep(v) => self.state.settings.sleep = v,
            Hit::ProfileSelect(i) => {
                if let Some(n) = self.profiles.get(i).cloned() {
                    self.profile_sel = Some(i);
                    self.name_input = Input::new(n);
                }
            }
            Hit::ProfileName => self.name_editing = true,
            Hit::ProfileSave => self.save_profile(),
            Hit::ProfileLoad => match self.selected_profile().map(String::from) {
                Some(n) => {
                    self.status = (Tone::Info, format!("Sending profile \"{n}\"..."));
                    self.pending = Some(Pending::LoadProfile(n));
                }
                None => self.status = (Tone::Err, "First click a profile in the list.".into()),
            },
            Hit::ProfileDelete => match self.selected_profile().map(String::from) {
                Some(n) => {
                    self.status = (Tone::Info, format!("Delete profile \"{n}\" from this PC?"));
                    self.confirm = Some(Confirm::DeleteProfile(n));
                }
                None => self.status = (Tone::Err, "First click a profile in the list.".into()),
            },
            Hit::OpenFolder => {
                let dir = state::profile_dir();
                let opened = std::fs::create_dir_all(&dir).and_then(|_| std::process::Command::new("explorer").arg(&dir).spawn());
                self.status = match opened {
                    Ok(_) => (Tone::Info, format!("Opened {}", dir.display())),
                    Err(e) => (Tone::Err, format!("Could not open {}: {e}", dir.display())),
                };
            }
            Hit::FactoryReset => {
                self.status = (Tone::Info, "Erase every change on the keyboard?".into());
                self.confirm = Some(Confirm::FactoryReset);
            }
            Hit::Confirm(yes) => {
                let Some(c) = self.confirm.take() else { return };
                if !yes {
                    self.status = (Tone::Info, "Cancelled.".into());
                    return;
                }
                match c {
                    Confirm::FactoryReset => {
                        self.status = (Tone::Info, "Resetting the keyboard...".into());
                        self.pending = Some(Pending::FactoryReset);
                    }
                    Confirm::DeleteProfile(n) => {
                        let res = state::profile_path(&n).and_then(|p| std::fs::remove_file(&p).map_err(Into::into));
                        self.status = match res {
                            Ok(()) => (Tone::Ok, format!("Profile \"{n}\" deleted.")),
                            Err(e) => (Tone::Err, format!("Not deleted: {e:#}.")),
                        };
                        self.refresh_profiles(None);
                        self.name_input = Input::default();
                    }
                }
            }
        }
    }

    pub fn hit_at(&self, x: u16, y: u16) -> Option<Hit> {
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
            Some(Hit::MacroSelect(_)) => {
                let n = (self.macro_sel as i16 - d as i16).clamp(1, 100) as u8;
                self.on_hit(Hit::MacroSelect(n));
            }
            Some(Hit::Step(l, _)) => self.step(l, d),
            Some(Hit::Channel(t, ch, _)) => self.on_hit(Hit::Channel(t, ch, d as i16 * 8)),
            _ => {}
        }
    }

    fn on_key(&mut self, code: KeyCode, mods: KeyModifiers) {
        if self.macro_editing {
            match code {
                KeyCode::Enter => self.save_macro(),
                KeyCode::Esc => {
                    self.macro_editing = false;
                    self.load_macro_text();
                    self.status = (Tone::Info, "Edit cancelled.".into());
                }
                other => self.macro_input.key(other),
            }
            return;
        }
        if self.name_editing {
            match code {
                KeyCode::Enter => self.save_profile(),
                KeyCode::Esc => {
                    self.name_editing = false;
                    self.status = (Tone::Info, "Cancelled.".into());
                }
                other => self.name_input.key(other),
            }
            return;
        }
        if self.confirm.is_some() && code == KeyCode::Esc {
            self.on_hit(Hit::Confirm(false));
            return;
        }
        if self.choosing_key && code == KeyCode::Esc {
            self.choosing_key = false;
            self.status = (Tone::Info, "Cancelled.".into());
            return;
        }
        match code {
            // Esc never quits: people press it to cancel things.
            KeyCode::Char('q') | KeyCode::Char('Q') => self.quit = true,
            KeyCode::Char('c') if mods.contains(KeyModifiers::CONTROL) => self.quit = true,
            KeyCode::Char(c @ '1'..='6') => self.tab = Tab::ALL[(c as u8 - b'1') as usize],
            KeyCode::Tab => {
                let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
                self.tab = Tab::ALL[(i + 1) % Tab::ALL.len()];
            }
            KeyCode::BackTab => {
                let i = Tab::ALL.iter().position(|t| *t == self.tab).unwrap_or(0);
                self.tab = Tab::ALL[(i + Tab::ALL.len() - 1) % Tab::ALL.len()];
            }
            KeyCode::Enter | KeyCode::Char('a') if self.tab.sends() => self.on_hit(Hit::Apply),
            KeyCode::Up if self.tab == Tab::Lighting => {
                let i = self.lighting.effect.id() as usize;
                self.select_effect(i.saturating_sub(1));
            }
            KeyCode::Down if self.tab == Tab::Lighting => {
                let i = self.lighting.effect.id() as usize;
                self.select_effect((i + 1).min(Effect::ALL.len() - 1));
            }
            KeyCode::Up if self.tab == Tab::Macros => self.on_hit(Hit::MacroSelect(self.macro_sel.saturating_sub(1).max(1))),
            KeyCode::Down if self.tab == Tab::Macros => self.on_hit(Hit::MacroSelect((self.macro_sel + 1).min(100))),
            KeyCode::Up | KeyCode::Down | KeyCode::Left | KeyCode::Right if self.tab == Tab::Keys => {
                if self.choosing_key {
                    self.status = (Tone::Info, "Click the new key with the mouse, or press Esc to cancel.".into());
                } else if let Some(k) = next_key(self.map_key, code) {
                    self.on_hit(Hit::MapKey(k.index));
                }
            }
            KeyCode::Left | KeyCode::Right => {
                let d = if code == KeyCode::Left { -1 } else { 1 };
                match self.tab {
                    Tab::KeyColors => self.step(Level::KeyBrightness, d),
                    Tab::Lighting => self.step(Level::Brightness, d),
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Key reached from `from` with an arrow key on the keyboard picture.
fn next_key(from: Option<&'static layout::Key>, code: KeyCode) -> Option<&'static layout::Key> {
    let Some(from) = from else { return KEYS.iter().find(|k| k.id == "Esc") };
    let center = |k: &layout::Key| 2 * k.x4 as i32 + k.w4 as i32;
    match code {
        KeyCode::Left | KeyCode::Right => {
            let row = KEYS.iter().filter(|k| k.row == from.row);
            if code == KeyCode::Left {
                row.filter(|k| k.x4 < from.x4).max_by_key(|k| k.x4)
            } else {
                row.filter(|k| k.x4 > from.x4).min_by_key(|k| k.x4)
            }
        }
        KeyCode::Up | KeyCode::Down => {
            let row = if code == KeyCode::Up { from.row.checked_sub(1)? } else { from.row + 1 };
            KEYS.iter().filter(|k| k.row == row).min_by_key(|k| (center(k) - center(from)).abs())
        }
        _ => None,
    }
}

fn draw(f: &mut Frame, app: &mut App) {
    pages::draw(f, app);
}

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
        if let Some(what) = app.pending.take() {
            app.apply_now(what);
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
            Event::Paste(s) if app.macro_editing => app.macro_input.insert_str(&s),
            Event::Paste(s) if app.name_editing => app.name_input.insert_str(&s),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::Down(MouseButton::Left) => {
                    dragging = true;
                    match app.hit_at(m.column, m.row) {
                        Some(h) => app.on_hit(h),
                        None => {
                            app.macro_editing = false;
                            app.name_editing = false;
                        }
                    }
                }
                MouseEventKind::Drag(MouseButton::Left) if dragging => {
                    if let Some(h @ Hit::PaintKey(_)) = app.hit_at(m.column, m.row) {
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
mod tests;
