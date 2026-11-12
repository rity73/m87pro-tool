//! Settings the keyboard cannot report back, saved on this PC.
//!
//! Plain text, one `name = value` per line, so it can be read, edited and
//! backed up by hand:
//!
//! ```text
//! settings.disable-win = on
//! settings.sleep = 2
//! base.CapsLock = key:Ctrl
//! fn.Pause = media:mute
//! ```

use std::collections::BTreeMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use m87_proto::keymap::{KeyAction, Layer};
use m87_proto::layout::{self, KEYS};
use m87_proto::macros::{self, Macro};
use m87_proto::settings::Settings;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct State {
    pub settings: Settings,
    /// Non-default key actions, by (layer, key id).
    pub keys: BTreeMap<(u8, &'static str), KeyAction>,
    /// Macros by number (1..=100).
    pub macros: BTreeMap<u8, Macro>,
}

fn layer_code(l: Layer) -> u8 {
    match l {
        Layer::Base => 0,
        Layer::Fn => 1,
    }
}

pub fn path() -> PathBuf {
    let base = std::env::var_os("APPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    base.join("m87pro-tool").join("state.txt")
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
            Ok(text) => State::parse(&text).with_context(|| format!("could not read {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(State::default()),
            Err(e) => Err(e).with_context(|| format!("could not open {}", p.display())),
        }
    }

    pub fn save(&self) -> Result<()> {
        let p = path();
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("could not create {}", dir.display()))?;
        }
        std::fs::write(&p, self.to_text()).with_context(|| format!("could not write {}", p.display()))
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
            "# M87 Pro settings last sent by m87. The keyboard cannot report its settings,\n\
             # so this file is how m87 remembers them. Edit with care.\n",
        );
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
        let text = st.to_text();
        assert!(text.contains("fn.Pause = media:mute"));
        assert_eq!(State::parse(&text).unwrap(), st);
        assert_eq!(st.entries(Layer::Fn), vec![(115, KeyAction::Media(0xE2))]);
        st.set_key(Layer::Fn, "Pause", KeyAction::Default);
        assert!(st.entries(Layer::Fn).is_empty());
    }

    #[test]
    fn errors_name_the_line() {
        let e = State::parse("settings.sleep = x").unwrap_err().to_string();
        assert!(e.contains("line 1"), "{e}");
        let e = State::parse("\nbase.Nope = default").unwrap_err().to_string();
        assert!(e.contains("line 2"), "{e}");
    }
}
