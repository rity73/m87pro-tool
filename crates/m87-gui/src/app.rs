//! Everything the window shows and changes, without any window code, so
//! it can be tested. The window reads this model and calls its methods.

use m87_core::state::{self, LightMode, State};
use m87_hid::Error as HidError;
use m87_proto::keymap::{KeyAction, Layer};
use m87_proto::layout::{self, KEYS, SLOTS};
use m87_proto::lighting::{Lighting, Rgb, MAX_LEVEL};
use m87_proto::macros::Macro;
use m87_proto::plan::{self, Packet};
use m87_proto::Link;

use crate::i18n::{tr, Lang, T};

pub const PALETTE: [(T, Rgb); 10] = [
    (T::PalRed, Rgb::new(0xFF, 0x00, 0x00)),
    (T::PalOrange, Rgb::new(0xFF, 0x80, 0x00)),
    (T::PalYellow, Rgb::new(0xFF, 0xFF, 0x00)),
    (T::PalGreen, Rgb::new(0x00, 0xFF, 0x00)),
    (T::PalCyan, Rgb::new(0x00, 0xFF, 0xFF)),
    (T::PalBlue, Rgb::new(0x00, 0x00, 0xFF)),
    (T::PalPurple, Rgb::new(0x80, 0x00, 0xFF)),
    (T::PalPink, Rgb::new(0xFF, 0x00, 0xFF)),
    (T::PalWhite, Rgb::new(0xFF, 0xFF, 0xFF)),
    (T::PalOff, Rgb::new(0x00, 0x00, 0x00)),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Lighting,
    KeyColors,
    Keys,
    Macros,
    Settings,
    Profiles,
}

impl Page {
    pub const ALL: [Page; 6] = [Page::Lighting, Page::KeyColors, Page::Keys, Page::Macros, Page::Settings, Page::Profiles];
    pub fn title(self) -> T {
        match self {
            Page::Lighting => T::TabLighting,
            Page::KeyColors => T::TabKeyColors,
            Page::Keys => T::TabKeys,
            Page::Macros => T::TabMacros,
            Page::Settings => T::TabSettings,
            Page::Profiles => T::TabProfiles,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Info,
    Ok,
    Err,
}

/// Packets of one send, for each connection type.
pub struct Packets {
    pub dongle: Vec<Packet>,
    pub wired: Vec<Packet>,
}

impl Packets {
    pub fn for_link(&self, l: Link) -> &[Packet] {
        match l {
            Link::Dongle => &self.dongle,
            Link::Wired => &self.wired,
        }
    }
}

/// One send to the keyboard.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Job {
    Lighting,
    KeyColors,
    Keys,
    ResetKeys,
    Macros,
    Settings,
    LoadProfile(String),
    FactoryReset,
}

pub struct App {
    pub lang: Lang,
    pub page: Page,
    pub lighting: Lighting,
    pub key_colors: [Rgb; SLOTS],
    pub brush: Rgb,
    pub key_brightness: u8,
    pub status: (Tone, String),
    pub link: Option<Link>,
    pub battery: Option<u8>,
    /// Draft of everything saved on this PC.
    pub state: State,
    /// Last state sent successfully.
    pub saved: State,
    pub layer: Layer,
    pub map_key: Option<&'static layout::Key>,
    pub choosing_key: bool,
    pub macro_sel: u8,
    pub profiles: Vec<String>,
    pub profile_sel: Option<usize>,
    /// The send in progress and, when it replaces everything, the new state.
    pub busy: Option<(Job, Option<State>)>,
}

impl App {
    pub fn blank(lang: Lang) -> Self {
        App {
            lang,
            page: Page::Lighting,
            lighting: Lighting::default(),
            key_colors: [Rgb::default(); SLOTS],
            brush: Rgb::new(0xFF, 0, 0),
            key_brightness: MAX_LEVEL,
            status: (Tone::Info, tr(lang, T::Ready).into()),
            link: None,
            battery: None,
            state: State::default(),
            saved: State::default(),
            layer: Layer::Base,
            map_key: None,
            choosing_key: false,
            macro_sel: 1,
            profiles: Vec::new(),
            profile_sel: None,
            busy: None,
        }
    }

    /// Load what is saved on this PC.
    pub fn load(lang: Lang) -> Self {
        let mut app = App::blank(lang);
        match State::load() {
            Ok(st) => {
                app.state = st.clone();
                app.saved = st;
            }
            Err(e) => app.status = (Tone::Err, format!("{} {e:#}", app.t(T::LoadFailed))),
        }
        app.sync_from_state();
        app.profiles = state::list_profiles();
        app
    }

    pub fn t(&self, t: T) -> &'static str {
        tr(self.lang, t)
    }

    fn info(&mut self, t: T) {
        self.status = (Tone::Info, self.t(t).into());
    }

    fn err(&mut self, t: T) {
        self.status = (Tone::Err, self.t(t).into());
    }

    pub fn sync_from_state(&mut self) {
        self.lighting = self.state.lighting;
        self.key_colors = self.state.color_table();
        self.key_brightness = self.state.key_brightness;
    }

    pub fn link_name(&self, l: Link) -> &'static str {
        match l {
            Link::Dongle => self.t(T::LinkDongle),
            Link::Wired => self.t(T::LinkWired),
        }
    }

    // ------------------------------------------------------------ sending

    /// Unsent changes on the current page.
    pub fn unsent(&self) -> bool {
        match self.page {
            Page::Lighting => self.saved.light_mode != LightMode::Effect || self.saved.lighting != self.lighting,
            Page::KeyColors => {
                self.saved.light_mode != LightMode::KeyColors
                    || self.saved.color_table() != self.key_colors
                    || self.saved.key_brightness != self.key_brightness
            }
            Page::Keys => [Layer::Base, Layer::Fn].iter().any(|l| self.state.entries(*l) != self.saved.entries(*l)),
            Page::Settings => self.state.settings != self.saved.settings,
            _ => false,
        }
    }

    /// The job behind the "Apply to keyboard" button of the current page.
    pub fn page_job(&self) -> Option<Job> {
        Some(match self.page {
            Page::Lighting => Job::Lighting,
            Page::KeyColors => Job::KeyColors,
            Page::Keys => Job::Keys,
            Page::Macros => Job::Macros,
            Page::Settings => Job::Settings,
            Page::Profiles => return None,
        })
    }

    fn colors_list(&self) -> Vec<(u8, Rgb)> {
        KEYS.iter().map(|k| (k.index, self.key_colors[k.index as usize])).collect()
    }

    /// Packets for a job, and the complete new state for jobs that replace
    /// everything. Errors are ready to show.
    pub fn plan(&self, job: &Job, link: Link) -> Result<(Vec<Packet>, Option<State>), String> {
        Ok(match job {
            Job::Lighting => (plan::set_effect(link, &self.lighting), None),
            Job::KeyColors => (plan::set_key_colors(link, self.key_brightness, &self.colors_list()), None),
            Job::Keys => {
                let mut p = Vec::new();
                for layer in [Layer::Base, Layer::Fn] {
                    if self.state.entries(layer) != self.saved.entries(layer) {
                        p.extend(plan::set_key_map(link, layer, &self.state.entries(layer)));
                    }
                }
                if p.is_empty() {
                    return Err(self.t(T::NoKeyChanges).into());
                }
                (p, None)
            }
            Job::ResetKeys => (plan::reset_key_maps(link), None),
            Job::Macros => (plan::set_macros(link, &self.state.macro_storage()?), None),
            Job::Settings => (plan::set_settings(link, &self.state.settings), None),
            Job::LoadProfile(name) => {
                let st = State::load_profile(name).map_err(|e| format!("{e:#}"))?;
                (st.full_plan(link)?, Some(st))
            }
            Job::FactoryReset => (plan::factory_reset(link), Some(State::default())),
        })
    }

    /// Prepare a send. Returns the packets for each connection type; the
    /// worker thread uses the set that matches the keyboard it opens.
    pub fn start(&mut self, job: Job) -> Option<Packets> {
        if self.busy.is_some() {
            self.err(T::Busy);
            return None;
        }
        let built = self.plan(&job, Link::Dongle).and_then(|(d, whole)| Ok((d, self.plan(&job, Link::Wired)?.0, whole)));
        match built {
            Ok((dongle, wired, whole)) => {
                self.busy = Some((job, whole));
                self.info(T::Sending);
                Some(Packets { dongle, wired })
            }
            Err(e) => {
                self.status = (Tone::Err, format!("{} {e}", self.t(T::NotApplied)));
                None
            }
        }
    }

    pub fn hid_error(&self, e: &HidError) -> String {
        let t = match e {
            HidError::NotFound => T::ErrNotFound,
            HidError::KeyboardOffline => T::ErrOffline,
            HidError::NoResponse(_) => T::ErrNoResponse,
            HidError::OpenFailed(..) => T::ErrOpen,
            HidError::Io(_) => T::ErrIo,
            other => return format!("{other}."),
        };
        self.t(t).into()
    }

    /// The worker finished.
    pub fn finish(&mut self, result: Result<Link, HidError>) {
        let Some((job, whole)) = self.busy.take() else { return };
        let link = match result {
            Ok(l) => l,
            Err(e) => {
                self.status = (Tone::Err, format!("{} {}", self.t(T::NotApplied), self.hid_error(&e)));
                return;
            }
        };
        self.link = Some(link);
        let msg = match &job {
            Job::Lighting => self.t(T::EffectSent).to_string(),
            Job::KeyColors => self.t(T::KeyColorsSent).to_string(),
            Job::Keys => self.t(T::KeysSent).to_string(),
            Job::ResetKeys => self.t(T::ResetDone).to_string(),
            Job::Macros => self.t(T::MacrosSent).to_string(),
            Job::Settings => self.t(T::SettingsSent).to_string(),
            Job::LoadProfile(n) => format!("{} {n}", self.t(T::ProfileLoaded)),
            Job::FactoryReset => self.t(T::FactoryDone).to_string(),
        };
        if let Some(st) = whole {
            self.state = st.clone();
            self.saved = st;
            self.sync_from_state();
        } else {
            match job {
                Job::ResetKeys => {
                    self.state.clear_keys();
                    self.saved.keys.clear();
                }
                Job::Keys => self.saved.keys = self.state.keys.clone(),
                Job::Macros => self.saved.macros = self.state.macros.clone(),
                Job::Settings => self.saved.settings = self.state.settings,
                Job::Lighting => {
                    for st in [&mut self.state, &mut self.saved] {
                        st.light_mode = LightMode::Effect;
                        st.lighting = self.lighting;
                    }
                }
                Job::KeyColors => {
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
            Err(e) => format!(" ({} {e:#})", self.t(T::SaveFailed)),
        };
        let applied = format!("{} {}.", self.t(T::Applied), self.link_name(link));
        self.status = (Tone::Ok, format!("{applied} {msg}{note}"));
    }

    // ------------------------------------------------------------ lighting

    pub fn set_effect_color(&mut self, c: Rgb) {
        self.lighting.color = c;
        self.lighting.rainbow = false;
    }

    pub fn paint(&mut self, index: u8) {
        if let Some(c) = self.key_colors.get_mut(index as usize) {
            *c = self.brush;
        }
    }

    pub fn paint_all(&mut self, c: Rgb) {
        for k in KEYS {
            self.key_colors[k.index as usize] = c;
        }
    }

    pub fn lit_count(&self) -> usize {
        KEYS.iter().filter(|k| self.key_colors[k.index as usize] != Rgb::default()).count()
    }

    // ------------------------------------------------------------ key mapping

    pub fn current_action(&self) -> Option<KeyAction> {
        self.map_key.map(|k| self.state.key(self.layer, k.id))
    }

    pub fn action_text(&self, a: KeyAction) -> String {
        match a {
            KeyAction::Default => self.t(T::NormalFunction).into(),
            KeyAction::Disabled => self.t(T::DisabledText).into(),
            KeyAction::Mouse(m) => crate::i18n::mouse_name(self.lang, m).into(),
            KeyAction::Media(u) => m87_proto::keymap::MEDIA
                .iter()
                .find(|(_, v)| *v == u)
                .map(|(id, _)| crate::i18n::media_name(self.lang, id).to_string())
                .unwrap_or_else(|| a.to_text()),
            KeyAction::Macro { slot, .. } => format!("{} {}", self.t(T::Macro), slot + 1),
            KeyAction::Key { .. } => a.to_text().trim_start_matches("key:").to_string(),
        }
    }

    /// A key on the picture was clicked.
    pub fn click_key(&mut self, index: u8) {
        let Some(key) = KEYS.iter().find(|k| k.index == index) else { return };
        if self.choosing_key {
            self.choosing_key = false;
            if key.id == "Fn" {
                self.err(T::FnCannotSend);
                return;
            }
            let modifiers = match self.current_action() {
                Some(KeyAction::Key { modifiers, .. }) => modifiers,
                _ => 0,
            };
            self.set_action(KeyAction::Key { modifiers, usage: key.usage });
        } else {
            self.map_key = Some(key);
            if key.id == "Fn" {
                self.err(T::FnFixed);
            } else {
                self.status = (Tone::Info, format!("{} {}", self.t(T::Selected), key.id));
            }
        }
    }

    pub fn set_action(&mut self, a: KeyAction) {
        self.choosing_key = false;
        let Some(k) = self.map_key else {
            self.err(T::FirstClickKey);
            return;
        };
        if k.id == "Fn" {
            self.err(T::FnFixed);
            return;
        }
        self.state.set_key(self.layer, k.id, a);
        self.status = (Tone::Info, format!("{} {} {}. {}", k.id, self.t(T::WillDo), self.action_text(a), self.t(T::ClickApply)));
    }

    pub fn toggle_modifier(&mut self, bit: u8) {
        match self.current_action() {
            Some(KeyAction::Key { modifiers, usage }) if usage != 0 && !(0xE0..=0xE7).contains(&usage) => {
                self.set_action(KeyAction::Key { modifiers: modifiers ^ bit, usage })
            }
            _ => self.err(T::ModNeedsKey),
        }
    }

    pub fn choose_key(&mut self) {
        match self.map_key {
            None => self.err(T::FirstClickKey),
            Some(k) if k.id == "Fn" => self.err(T::FnFixed),
            Some(_) => {
                self.choosing_key = true;
                self.info(T::ChooseKeyHint);
            }
        }
    }

    pub fn cancel(&mut self) {
        if self.choosing_key {
            self.choosing_key = false;
            self.info(T::Cancelled);
        }
    }

    // ------------------------------------------------------------ macros

    pub fn macro_text(&self, n: u8) -> String {
        self.state.macros.get(&n).map(|m| m.to_text()).unwrap_or_default()
    }

    /// Store the macro text for the selected number. Returns true when the
    /// macros should be sent.
    pub fn save_macro(&mut self, text: &str) -> bool {
        // Line breaks typed in the box count as separators.
        let text = text.replace("\r\n", ",").replace('\n', ",");
        let text = text.trim().trim_matches(',').trim();
        if text.is_empty() {
            self.err(T::TypeSteps);
            return false;
        }
        match Macro::parse(text) {
            Ok(m) => {
                let old = self.state.macros.insert(self.macro_sel, m);
                if let Err(e) = self.state.macro_storage() {
                    match old {
                        Some(o) => self.state.macros.insert(self.macro_sel, o),
                        None => self.state.macros.remove(&self.macro_sel),
                    };
                    self.status = (Tone::Err, format!("{} {e}", self.t(T::MacroNotSaved)));
                    return false;
                }
                true
            }
            Err(e) => {
                self.status = (Tone::Err, format!("{} {e}.", self.t(T::MacroNotSaved)));
                false
            }
        }
    }

    pub fn delete_macro(&mut self) -> bool {
        if self.state.macros.remove(&self.macro_sel).is_some() {
            true
        } else {
            self.err(T::MacroEmpty);
            false
        }
    }

    // ------------------------------------------------------------ profiles

    pub fn selected_profile(&self) -> Option<String> {
        self.profile_sel.and_then(|i| self.profiles.get(i)).cloned()
    }

    pub fn refresh_profiles(&mut self, select: Option<&str>) {
        self.profiles = state::list_profiles();
        self.profile_sel = select.and_then(|n| self.profiles.iter().position(|p| p.eq_ignore_ascii_case(n)));
    }

    pub fn save_profile(&mut self, name: &str) {
        let name = match state::check_profile_name(name) {
            Ok(n) => n.to_string(),
            Err(_) => return self.err(T::BadName),
        };
        match self.saved.save_profile(&name) {
            Ok(_) => {
                self.refresh_profiles(Some(&name));
                self.status = (Tone::Ok, format!("{} {name}", self.t(T::ProfileSaved)));
            }
            Err(e) => self.status = (Tone::Err, format!("{} {e:#}", self.t(T::NotSaved))),
        }
    }

    pub fn delete_profile(&mut self, name: &str) {
        let res = state::profile_path(name).and_then(|p| std::fs::remove_file(&p).map_err(Into::into));
        match res {
            Ok(()) => self.status = (Tone::Ok, format!("{} {name}", self.t(T::ProfileDeleted))),
            Err(e) => self.status = (Tone::Err, format!("{} {e:#}", self.t(T::NotSaved))),
        }
        self.refresh_profiles(None);
    }

    pub fn import_profile(&mut self, path: &std::path::Path) {
        let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("Imported").to_string();
        // Make the name valid: keep allowed characters only.
        let mut name: String = name.chars().filter(|c| c.is_alphanumeric() || " -_".contains(*c)).take(40).collect();
        if name.trim().is_empty() {
            name = "Imported".into();
        }
        match State::read_file(path).and_then(|st| st.save_profile(&name)) {
            Ok(_) => {
                self.refresh_profiles(Some(name.trim()));
                self.status = (Tone::Ok, format!("{} {}", self.t(T::ProfileAdded), name.trim()));
            }
            Err(e) => self.status = (Tone::Err, format!("{} {e:#}", self.t(T::NotSaved))),
        }
    }

    pub fn export_profile(&mut self, path: &std::path::Path) {
        let st = match self.selected_profile() {
            Some(n) => State::load_profile(&n),
            None => Ok(self.saved.clone()),
        };
        match st.and_then(|st| st.write_to(path)) {
            Ok(()) => self.status = (Tone::Ok, format!("{} {}", self.t(T::Exported), path.display())),
            Err(e) => self.status = (Tone::Err, format!("{} {e:#}", self.t(T::NotSaved))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> App {
        state::set_data_dir(std::env::temp_dir().join(format!("m87pro-gui-test-{}", std::process::id())));
        let mut a = App::blank(Lang::En);
        a.link = Some(Link::Dongle);
        a
    }

    #[test]
    fn key_mapping_flow() {
        let mut a = app();
        a.page = Page::Keys;
        a.click_key(55); // CapsLock
        a.choose_key();
        assert!(a.choosing_key);
        a.click_key(56); // A
        assert_eq!(a.state.key(Layer::Base, "CapsLock"), KeyAction::Key { modifiers: 0, usage: 0x04 });
        a.toggle_modifier(m87_proto::keymap::modifier::LCTRL);
        assert_eq!(a.state.key(Layer::Base, "CapsLock").to_text(), "key:Ctrl+A");
        assert!(a.unsent());
        let p = a.start(Job::Keys).unwrap();
        assert!(!p.dongle.is_empty() && !p.wired.is_empty());
        assert!(a.start(Job::Keys).is_none(), "second send while busy is refused");
        a.finish(Ok(Link::Dongle));
        assert!(!a.unsent());
        assert_eq!(a.status.0, Tone::Ok);
    }

    #[test]
    fn errors_are_translated() {
        let mut a = app();
        a.lang = Lang::Ko;
        a.start(Job::Lighting).unwrap();
        a.finish(Err(HidError::KeyboardOffline));
        assert_eq!(a.status.0, Tone::Err);
        assert!(a.status.1.contains("절전"), "{}", a.status.1);
        a.start(Job::Lighting).unwrap();
        a.finish(Err(HidError::NotFound));
        assert!(a.status.1.contains("찾지 못했습니다"));
    }

    #[test]
    fn macros_and_profiles() {
        let mut a = app();
        a.macro_sel = 3;
        assert!(a.save_macro("tap:H,\r\ntap:I"));
        assert_eq!(a.macro_text(3), "tap:H, tap:I");
        assert!(!a.save_macro("nonsense"));
        assert_eq!(a.status.0, Tone::Err);
        a.save_profile("Gui test");
        assert_eq!(a.selected_profile().as_deref(), Some("Gui test"));
        a.save_profile("bad/name");
        assert_eq!(a.status.0, Tone::Err);
        a.delete_profile("Gui test");
        assert!(a.selected_profile().is_none());
    }

    #[test]
    fn lighting_finish_updates_saved() {
        let mut a = app();
        a.set_effect_color(Rgb::new(0, 0, 255));
        assert!(!a.lighting.rainbow);
        a.start(Job::Lighting).unwrap();
        a.finish(Ok(Link::Dongle));
        assert_eq!(a.saved.lighting.color, Rgb::new(0, 0, 255));
        a.page = Page::KeyColors;
        a.paint(1);
        assert_eq!(a.lit_count(), 1);
        assert!(a.unsent());
    }
}
