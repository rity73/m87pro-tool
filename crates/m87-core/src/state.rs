//! Settings the keyboard cannot report back, saved on this PC.
//!
//! Plain text, one `name = value` per line, so it can be read, edited and
//! backed up by hand. Profiles use the same format.
//!
//! ```text
//! light.mode = effect
//! light.effect = rolling
//! color.Esc = FF0000
//! settings.disable-win = on
//! settings.sleep = 2
//! base.CapsLock = key:Ctrl
//! fn.Pause = media:mute
//! macro.1 = tap:H, tap:I
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use m87_proto::keymap::{KeyAction, Layer};
use m87_proto::layout::{self, KEYS, SLOTS};
use m87_proto::lighting::{Effect, Lighting, Rgb, MAX_LEVEL};
use m87_proto::macros::{self, Macro};
use m87_proto::plan::{self, Packet};
use m87_proto::settings::Settings;
use m87_proto::Link;

/// Which lighting the keyboard shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LightMode {
    /// A built-in effect (`light.*`).
    #[default]
    Effect,
    /// Fixed per-key colors (`color.*`).
    KeyColors,
}

#[derive(Debug, Clone, PartialEq)]
pub struct State {
    pub light_mode: LightMode,
    pub lighting: Lighting,
    /// Per-key colors by key id. Keys not listed are off.
    pub key_colors: BTreeMap<&'static str, Rgb>,
    pub key_brightness: u8,
    pub settings: Settings,
    /// Non-default key actions, by (layer, key id).
    pub keys: BTreeMap<(u8, &'static str), KeyAction>,
    /// Macros by number (1..=100).
    pub macros: BTreeMap<u8, Macro>,
}

impl Default for State {
    fn default() -> Self {
        State {
            light_mode: LightMode::Effect,
            lighting: Lighting::default(),
            key_colors: BTreeMap::new(),
            key_brightness: MAX_LEVEL,
            settings: Settings::default(),
            keys: BTreeMap::new(),
            macros: BTreeMap::new(),
        }
    }
}

fn layer_code(l: Layer) -> u8 {
    match l {
        Layer::Base => 0,
        Layer::Fn => 1,
    }
}

static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Use another folder instead of `%APPDATA%\m87pro-tool` (for tests).
/// Only the first call has an effect.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

/// Folder with everything saved on this PC.
pub fn data_dir() -> PathBuf {
    if let Some(d) = DATA_DIR.get() {
        return d.clone();
    }
    if cfg!(test) {
        // Tests never touch the real settings.
        return std::env::temp_dir().join(format!("m87pro-tool-test-{}", std::process::id()));
    }
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("m87pro-tool")
}

/// File with the settings last sent to the keyboard.
pub fn path() -> PathBuf {
    data_dir().join("state.txt")
}

/// Folder with the saved profiles (one `NAME.txt` each).
pub fn profile_dir() -> PathBuf {
    data_dir().join("profiles")
}

/// Check a profile name: 1-40 letters, digits, spaces, `-` or `_`.
pub fn check_profile_name(name: &str) -> Result<&str> {
    let name = name.trim();
    let ok = !name.is_empty()
        && name.chars().count() <= 40
        && name.chars().all(|c| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_');
    if !ok {
        anyhow::bail!("\"{name}\" cannot be used as a profile name. Use 1 to 40 letters, digits, spaces, - or _");
    }
    Ok(name)
}

pub fn profile_path(name: &str) -> Result<PathBuf> {
    Ok(profile_dir().join(format!("{}.txt", check_profile_name(name)?)))
}

/// Names of the saved profiles, sorted.
pub fn list_profiles() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(profile_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension()? != "txt" {
                return None;
            }
            p.file_stem()?.to_str().map(String::from)
        })
        .filter(|n| check_profile_name(n).is_ok())
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names
}

fn write_file(p: &std::path::Path, text: &str) -> Result<()> {
    if let Some(dir) = p.parent() {
        if !dir.as_os_str().is_empty() {
            std::fs::create_dir_all(dir).with_context(|| format!("could not create the folder {}", dir.display()))?;
        }
    }
    std::fs::write(p, text).with_context(|| format!("could not write {}", p.display()))
}

fn on_off(v: &str) -> Option<bool> {
    match v.to_ascii_lowercase().as_str() {
        "on" | "true" | "1" | "yes" => Some(true),
        "off" | "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

impl State {
    pub fn load() -> Result<State> {
        let p = path();
        match std::fs::read_to_string(&p) {
            Ok(text) => State::parse(&text).with_context(|| format!("the file {} has an error", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e).with_context(|| format!("could not open {}", p.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        write_file(&path(), &self.to_text())
    }

    /// Read a profile or exported file.
    pub fn read_file(p: &std::path::Path) -> Result<State> {
        let text = std::fs::read_to_string(p).with_context(|| {
            if p.exists() {
                format!("could not open {}", p.display())
            } else {
                format!("the file {} does not exist", p.display())
            }
        })?;
        State::parse(&text).with_context(|| format!("the file {} has an error", p.display()))
    }

    pub fn write_to(&self, p: &std::path::Path) -> Result<()> {
        write_file(p, &self.to_text())
    }

    pub fn load_profile(name: &str) -> Result<State> {
        let p = profile_path(name)?;
        if !p.exists() {
            anyhow::bail!("there is no profile named \"{}\". Run `m87 profile list` to see the names", name.trim());
        }
        State::read_file(&p)
    }

    pub fn save_profile(&self, name: &str) -> Result<PathBuf> {
        let p = profile_path(name)?;
        self.write_to(&p)?;
        Ok(p)
    }

    /// Every packet needed to put this whole state on the keyboard.
    pub fn full_plan(&self, link: Link) -> std::result::Result<Vec<Packet>, String> {
        let mut out = plan::set_settings(link, &self.settings);
        for layer in [Layer::Base, Layer::Fn] {
            out.extend(plan::set_key_map(link, layer, &self.entries(layer)));
        }
        out.extend(plan::set_macros(link, &self.macro_storage()?));
        out.extend(self.light_plan(link));
        Ok(out)
    }

    /// Packets for the lighting part only.
    pub fn light_plan(&self, link: Link) -> Vec<Packet> {
        match self.light_mode {
            LightMode::Effect => plan::set_effect(link, &self.lighting),
            LightMode::KeyColors => plan::set_key_colors(link, self.key_brightness, &self.color_list()),
        }
    }

    /// (table index, color) for every key, off included.
    pub fn color_list(&self) -> Vec<(u8, Rgb)> {
        KEYS.iter().map(|k| (k.index, self.key_colors.get(k.id).copied().unwrap_or_default())).collect()
    }

    /// Per-key colors as a table indexed by slot.
    pub fn color_table(&self) -> [Rgb; SLOTS] {
        let mut t = [Rgb::default(); SLOTS];
        for (i, c) in self.color_list() {
            t[i as usize] = c;
        }
        t
    }

    /// Store per-key colors from a slot-indexed table.
    pub fn set_color_table(&mut self, t: &[Rgb; SLOTS]) {
        self.key_colors = KEYS
            .iter()
            .filter(|k| t[k.index as usize] != Rgb::default())
            .map(|k| (k.id, t[k.index as usize]))
            .collect();
    }

    pub fn parse(text: &str) -> Result<State> {
        let mut st = State::default();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let (name, value) = line
                .split_once('=')
                .map(|(a, b)| (a.trim(), b.trim()))
                .with_context(|| format!("line {}: expected NAME = VALUE", n + 1))?;
            let bad = || format!("line {}: bad value \"{value}\" for {name}", n + 1);
            let num = |max: u8| value.parse::<u8>().ok().filter(|v| *v <= max).with_context(bad);
            if name.starts_with("light.") || name.starts_with("colors.") || name.starts_with("color.") {
                match name {
                    "light.mode" => {
                        st.light_mode = match value {
                            "effect" => LightMode::Effect,
                            "key-colors" => LightMode::KeyColors,
                            _ => anyhow::bail!(bad()),
                        }
                    }
                    "light.effect" => st.lighting.effect = Effect::from_name(value).with_context(bad)?,
                    "light.color" => st.lighting.color = Rgb::parse(value).with_context(bad)?,
                    "light.rainbow" => st.lighting.rainbow = on_off(value).with_context(bad)?,
                    "light.brightness" => st.lighting.brightness = num(MAX_LEVEL)?,
                    "light.speed" => st.lighting.speed = num(MAX_LEVEL)?,
                    "light.direction" => st.lighting.direction = num(255)?,
                    "colors.brightness" => st.key_brightness = num(MAX_LEVEL)?,
                    _ if name.starts_with("color.") => {
                        let id = &name[6..];
                        let key = layout::find(id).with_context(|| format!("line {}: unknown key {id}", n + 1))?;
                        let c = Rgb::parse(value).with_context(bad)?;
                        if c == Rgb::default() {
                            st.key_colors.remove(key.id);
                        } else {
                            st.key_colors.insert(key.id, c);
                        }
                    }
                    _ => anyhow::bail!("line {}: unknown name {name}", n + 1),
                }
                continue;
            }
            let s = &mut st.settings;
            match name {
                "settings.disable-alt-tab" => s.disable_alt_tab = on_off(value).with_context(bad)?,
                "settings.disable-alt-f4" => s.disable_alt_f4 = on_off(value).with_context(bad)?,
                "settings.disable-win" => s.disable_win = on_off(value).with_context(bad)?,
                "settings.fn-switch" => s.fn_switch = value.parse().ok().with_context(bad)?,
                "settings.sleep" => s.sleep = value.parse().ok().with_context(bad)?,
                "settings.response" => s.response = value.parse().ok().with_context(bad)?,
                _ if name.starts_with("macro.") => {
                    let n: u8 = name[6..].parse().ok().filter(|n| (1..=100).contains(n)).with_context(|| format!("line {}: macro number must be 1-100", n + 1))?;
                    let m = Macro::parse(value).map_err(|e| anyhow::anyhow!("line {}: {e}", n + 1))?;
                    st.macros.insert(n, m);
                }
                _ => {
                    let (layer, key) = name.split_once('.').with_context(|| format!("line {}: unknown name {name}", n + 1))?;
                    let layer = match layer {
                        "base" => Layer::Base,
                        "fn" => Layer::Fn,
                        _ => anyhow::bail!("line {}: unknown name {name}", n + 1),
                    };
                    let key = layout::find(key).with_context(|| format!("line {}: unknown key {key}", n + 1))?;
                    let action = KeyAction::parse(value).map_err(|e| anyhow::anyhow!("line {}: {e}", n + 1))?;
                    st.set_key(layer, key.id, action);
                }
            }
        }
        Ok(st)
    }

    pub fn to_text(&self) -> String {
        let s = &self.settings;
        let b = |v: bool| if v { "on" } else { "off" };
        let mut out = String::from(
            "# M87 Pro settings written by m87. The keyboard cannot report its settings,\n\
             # so this file is how m87 remembers them. Edit with care.\n",
        );
        let l = &self.lighting;
        let mode = match self.light_mode {
            LightMode::Effect => "effect",
            LightMode::KeyColors => "key-colors",
        };
        out += &format!("light.mode = {mode}\n");
        out += &format!("light.effect = {}\n", l.effect.name());
        out += &format!("light.color = {}\n", l.color.to_hex());
        out += &format!("light.rainbow = {}\n", b(l.rainbow));
        out += &format!("light.brightness = {}\n", l.brightness);
        out += &format!("light.speed = {}\n", l.speed);
        out += &format!("light.direction = {}\n", l.direction);
        out += &format!("colors.brightness = {}\n", self.key_brightness);
        for k in KEYS {
            if let Some(c) = self.key_colors.get(k.id) {
                out += &format!("color.{} = {}\n", k.id, c.to_hex());
            }
        }
        out += &format!("settings.disable-alt-tab = {}\n", b(s.disable_alt_tab));
        out += &format!("settings.disable-alt-f4 = {}\n", b(s.disable_alt_f4));
        out += &format!("settings.disable-win = {}\n", b(s.disable_win));
        out += &format!("settings.fn-switch = {}\n", s.fn_switch);
        out += &format!("settings.sleep = {}\n", s.sleep);
        out += &format!("settings.response = {}\n", s.response);
        for (n, m) in &self.macros {
            out += &format!("macro.{n} = {}
", m.to_text());
        }
        for ((layer, id), action) in &self.keys {
            let l = if *layer == 0 { "base" } else { "fn" };
            out += &format!("{l}.{id} = {}\n", action.to_text());
        }
        out
    }

    pub fn set_key(&mut self, layer: Layer, id: &'static str, action: KeyAction) {
        let k = (layer_code(layer), id);
        if action == KeyAction::Default {
            self.keys.remove(&k);
        } else {
            self.keys.insert(k, action);
        }
    }

    pub fn key(&self, layer: Layer, id: &str) -> KeyAction {
        self.keys
            .iter()
            .find(|((l, k), _)| *l == layer_code(layer) && k.eq_ignore_ascii_case(id))
            .map(|(_, a)| *a)
            .unwrap_or(KeyAction::Default)
    }

    /// Table entries (slot index, action) for one layer.
    pub fn entries(&self, layer: Layer) -> Vec<(u8, KeyAction)> {
        KEYS.iter()
            .map(|k| (k.index, self.key(layer, k.id)))
            .filter(|(_, a)| *a != KeyAction::Default)
            .collect()
    }

    /// Macro storage buffer for upload (slot i = macro number i+1).
    pub fn macro_storage(&self) -> std::result::Result<Vec<u8>, String> {
        let top = self.macros.keys().max().copied().unwrap_or(0) as usize;
        let list: Vec<Option<Macro>> = (1..=top).map(|n| self.macros.get(&(n as u8)).cloned()).collect();
        macros::storage(&list)
    }

    pub fn clear_keys(&mut self) {
        self.keys.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut st = State::default();
        st.settings.disable_win = true;
        st.settings.sleep = 3;
        st.set_key(Layer::Base, "CapsLock", KeyAction::parse("key:LCtrl").unwrap());
        st.set_key(Layer::Fn, "Pause", KeyAction::parse("media:mute").unwrap());
        st.macros.insert(2, Macro::parse("tap:H, tap:I").unwrap());
        st.light_mode = LightMode::KeyColors;
        st.lighting.effect = Effect::Breath;
        st.lighting.rainbow = false;
        st.key_brightness = 3;
        st.key_colors.insert("Esc", Rgb::new(1, 2, 3));
        let text = st.to_text();
        assert!(text.contains("color.Esc = 010203"), "{text}");
        assert!(text.contains("fn.Pause = media:mute"));
        assert_eq!(State::parse(&text).unwrap(), st);
        assert_eq!(st.entries(Layer::Fn), vec![(115, KeyAction::Media(0xE2))]);
        st.set_key(Layer::Fn, "Pause", KeyAction::Default);
        assert!(st.entries(Layer::Fn).is_empty());
    }

    #[test]
    fn old_files_still_load() {
        let st = State::parse("settings.sleep = 3").unwrap();
        assert_eq!(st.settings.sleep, 3);
        assert_eq!(st.lighting, Lighting::default());
        assert_eq!(st.light_mode, LightMode::Effect);
    }

    #[test]
    fn profile_names() {
        assert!(check_profile_name("Gaming 2").is_ok());
        assert!(check_profile_name("work_v-1").is_ok());
        for bad in ["", "  ", "a/b", "..", "x:y", &"a".repeat(41)] {
            assert!(check_profile_name(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn full_plan_covers_everything() {
        let mut st = State::default();
        st.macros.insert(1, Macro::parse("tap:A").unwrap());
        let p = st.full_plan(Link::Dongle).unwrap();
        let cmds: Vec<u8> = p.iter().map(|x| x.bytes[1]).collect();
        for c in [0x07, 0x10, 0x12, 0x09, 0x05] {
            assert!(cmds.contains(&c), "{c:02X}");
        }
        st.light_mode = LightMode::KeyColors;
        assert!(st.full_plan(Link::Dongle).unwrap().iter().any(|x| x.bytes[1] == 0x14));
    }

    #[test]
    fn errors_name_the_line() {
        let e = State::parse("settings.sleep = x").unwrap_err().to_string();
        assert!(e.contains("line 1"), "{e}");
        let e = State::parse("\nbase.Nope = default").unwrap_err().to_string();
        assert!(e.contains("line 2"), "{e}");
    }
}
