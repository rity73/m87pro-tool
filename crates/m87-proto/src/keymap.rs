//! Key actions and key-map tables.
//!
//! Every key slot holds a 4-byte action. `00 00 00 00` keeps the key's
//! factory function. Encodings come from the original app's encoder
//! (`FUN_004161b0`); see `docs/protocol.md` section 4.

use crate::layout::{self, SLOTS};
use crate::BLOCK_TRAILER;

/// HID modifier bits (byte 1 of a keyboard action).
pub mod modifier {
    pub const LCTRL: u8 = 0x01;
    pub const LSHIFT: u8 = 0x02;
    pub const LALT: u8 = 0x04;
    pub const LWIN: u8 = 0x08;
    pub const RCTRL: u8 = 0x10;
    pub const RSHIFT: u8 = 0x20;
    pub const RALT: u8 = 0x40;
    pub const RWIN: u8 = 0x80;
}

const MODIFIER_NAMES: [(&str, u8); 12] = [
    ("Ctrl", modifier::LCTRL),
    ("Shift", modifier::LSHIFT),
    ("Alt", modifier::LALT),
    ("Win", modifier::LWIN),
    ("LCtrl", modifier::LCTRL),
    ("LShift", modifier::LSHIFT),
    ("LAlt", modifier::LALT),
    ("LWin", modifier::LWIN),
    ("RCtrl", modifier::RCTRL),
    ("RShift", modifier::RSHIFT),
    ("RAlt", modifier::RALT),
    ("RWin", modifier::RWIN),
];

/// Mouse functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mouse {
    Left,
    Right,
    Middle,
    Back,
    Forward,
    WheelUp,
    WheelDown,
}

impl Mouse {
    pub const ALL: [Mouse; 7] =
        [Mouse::Left, Mouse::Right, Mouse::Middle, Mouse::Back, Mouse::Forward, Mouse::WheelUp, Mouse::WheelDown];

    pub fn name(self) -> &'static str {
        match self {
            Mouse::Left => "left-click",
            Mouse::Right => "right-click",
            Mouse::Middle => "middle-click",
            Mouse::Back => "back",
            Mouse::Forward => "forward",
            Mouse::WheelUp => "wheel-up",
            Mouse::WheelDown => "wheel-down",
        }
    }

    fn bytes(self) -> [u8; 4] {
        match self {
            Mouse::Left => [0x01, 0x01, 0x01, 0],
            Mouse::Right => [0x01, 0x01, 0x02, 0],
            Mouse::Middle => [0x01, 0x01, 0x04, 0],
            Mouse::Back => [0x01, 0x01, 0x08, 0],
            Mouse::Forward => [0x01, 0x01, 0x10, 0],
            Mouse::WheelUp => [0x01, 0x03, 0x01, 0],
            Mouse::WheelDown => [0x01, 0x03, 0xFF, 0],
        }
    }
}

/// Media and system functions (HID consumer usages).
pub const MEDIA: [(&str, u16); 17] = [
    ("play-pause", 0x00CD),
    ("stop", 0x00B7),
    ("previous-track", 0x00B6),
    ("next-track", 0x00B5),
    ("volume-up", 0x00E9),
    ("volume-down", 0x00EA),
    ("mute", 0x00E2),
    ("media-player", 0x0183),
    ("mail", 0x018A),
    ("calculator", 0x0192),
    ("file-explorer", 0x0194),
    ("browser-home", 0x0223),
    ("browser-search", 0x0221),
    ("browser-back", 0x0224),
    ("browser-forward", 0x0225),
    ("browser-refresh", 0x0227),
    ("browser-favorites", 0x022A),
];

/// What a key does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    /// The key's factory function.
    Default,
    /// The key does nothing.
    Disabled,
    /// Send a key, optionally with modifiers (for example Ctrl+C).
    /// `usage` 0 with modifiers = modifier keys only.
    Key { modifiers: u8, usage: u8 },
    Mouse(Mouse),
    /// HID consumer usage (media / system).
    Media(u16),
    /// Play stored macro number `slot` (0-based, upload order).
    Macro { slot: u8, mode: u8, count: u8 },
}

impl KeyAction {
    /// 4-byte encoding stored in the key-map table.
    pub fn encode(self) -> [u8; 4] {
        match self {
            KeyAction::Default => [0, 0, 0, 0],
            KeyAction::Disabled => [0x05, 0x03, 0, 0],
            KeyAction::Key { modifiers, usage } => {
                // A lone modifier key is sent as its modifier bit.
                if modifiers == 0 && (0xE0..=0xE7).contains(&usage) {
                    [0x02, 1 << (usage - 0xE0), 0, 0]
                } else {
                    [0x02, modifiers, usage, 0]
                }
            }
            KeyAction::Mouse(m) => m.bytes(),
            KeyAction::Media(u) => [0x03, u as u8, (u >> 8) as u8, 0],
            KeyAction::Macro { slot, mode, count } => [0x06, slot, mode, count],
        }
    }

    /// Parse the text form used by the CLI and saved files:
    /// `default`, `disabled`, `key:Ctrl+Shift+Esc`, `mouse:left-click`,
    /// `media:volume-up`, `macro:3`.
    pub fn parse(s: &str) -> Result<KeyAction, String> {
        let s = s.trim();
        let (kind, arg) = s.split_once(':').unwrap_or((s, ""));
        match kind.to_ascii_lowercase().as_str() {
            "default" => Ok(KeyAction::Default),
            "disabled" | "disable" | "off" => Ok(KeyAction::Disabled),
            "key" => parse_combo(arg),
            "mouse" => Mouse::ALL
                .into_iter()
                .find(|m| m.name().eq_ignore_ascii_case(arg))
                .map(KeyAction::Mouse)
                .ok_or_else(|| format!("unknown mouse function \"{arg}\"; use one of: {}", mouse_names())),
            "media" => MEDIA
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(arg))
                .map(|(_, u)| KeyAction::Media(*u))
                .ok_or_else(|| format!("unknown media function \"{arg}\"; use one of: {}", media_names())),
            "macro" => {
                let n: u8 = arg.parse().map_err(|_| format!("\"{arg}\" is not a macro number (1-100)"))?;
                if !(1..=100).contains(&n) {
                    return Err("macro numbers go from 1 to 100".into());
                }
                Ok(KeyAction::Macro { slot: n - 1, mode: 0, count: 1 })
            }
            _ => Err(format!(
                "\"{s}\" is not an action. Use default, disabled, key:<KEYS>, mouse:<FUNCTION>, media:<FUNCTION> or macro:<NUMBER>"
            )),
        }
    }

    /// Text form; `parse(to_text())` gives the same action.
    pub fn to_text(self) -> String {
        match self {
            KeyAction::Default => "default".into(),
            KeyAction::Disabled => "disabled".into(),
            KeyAction::Key { modifiers, usage } => {
                let mut parts: Vec<&str> = Vec::new();
                for (name, bit) in &MODIFIER_NAMES[..4] {
                    if modifiers & bit != 0 {
                        parts.push(name);
                    }
                }
                for (name, bit) in &MODIFIER_NAMES[8..] {
                    if modifiers & bit != 0 {
                        parts.push(name);
                    }
                }
                let key_name;
                if usage != 0 {
                    key_name = usage_name(usage);
                    parts.push(&key_name);
                }
                format!("key:{}", parts.join("+"))
            }
            KeyAction::Mouse(m) => format!("mouse:{}", m.name()),
            KeyAction::Media(u) => match MEDIA.iter().find(|(_, x)| *x == u) {
                Some((n, _)) => format!("media:{n}"),
                None => format!("media:0x{u:04X}"),
            },
            KeyAction::Macro { slot, .. } => format!("macro:{}", slot as u16 + 1),
        }
    }
}

fn mouse_names() -> String {
    Mouse::ALL.iter().map(|m| m.name()).collect::<Vec<_>>().join(", ")
}

fn media_names() -> String {
    MEDIA.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
}

/// Extra keys that are not on the M87 Pro but can be sent.
const EXTRA_KEYS: [(&str, u8); 12] = [
    ("F13", 0x68),
    ("F14", 0x69),
    ("F15", 0x6A),
    ("F16", 0x6B),
    ("F17", 0x6C),
    ("F18", 0x6D),
    ("F19", 0x6E),
    ("F20", 0x6F),
    ("F21", 0x70),
    ("F22", 0x71),
    ("F23", 0x72),
    ("F24", 0x73),
];

fn usage_of(name: &str) -> Option<u8> {
    if let Some(k) = layout::find(name) {
        if k.usage != 0xAF {
            return Some(k.usage);
        }
    }
    EXTRA_KEYS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, u)| *u)
}

fn usage_name(usage: u8) -> String {
    layout::KEYS
        .iter()
        .find(|k| k.usage == usage && k.usage != 0xAF)
        .map(|k| k.id.to_string())
        .or_else(|| EXTRA_KEYS.iter().find(|(_, u)| *u == usage).map(|(n, _)| n.to_string()))
        .unwrap_or_else(|| format!("0x{usage:02X}"))
}

fn parse_combo(arg: &str) -> Result<KeyAction, String> {
    if arg.is_empty() {
        return Err("give a key after \"key:\", for example key:A or key:Ctrl+C".into());
    }
    let parts: Vec<&str> = arg.split('+').map(str::trim).collect();
    let mut modifiers = 0u8;
    let mut usage = 0u8;
    for (i, p) in parts.iter().enumerate() {
        let last = i == parts.len() - 1;
        if let Some((_, bit)) = MODIFIER_NAMES.iter().find(|(n, _)| n.eq_ignore_ascii_case(p)) {
            if last && parts.len() == 1 {
                // A single modifier key on its own, e.g. key:LShift.
                usage = 0xE0 + bit.trailing_zeros() as u8;
            } else {
                modifiers |= bit;
            }
        } else if last {
            usage = usage_of(p).ok_or_else(|| format!("unknown key \"{p}\". Run `m87 keys` to see key names"))?;
        } else {
            return Err(format!("\"{p}\" is not a modifier. Use Ctrl, Shift, Alt or Win before the last key"));
        }
    }
    Ok(KeyAction::Key { modifiers, usage })
}

/// Which key layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Layer {
    /// Normal layer.
    Base,
    /// While Fn is held.
    Fn,
}

impl Layer {
    pub fn name(self) -> &'static str {
        match self {
            Layer::Base => "base",
            Layer::Fn => "fn",
        }
    }
}

pub const TABLE_LEN: usize = SLOTS * 4;

/// Full key-map table for one layer: `action` at `index*4`, `AA 55` at the end.
pub fn table(entries: &[(u8, KeyAction)]) -> [u8; TABLE_LEN] {
    let mut t = [0u8; TABLE_LEN];
    for &(index, action) in entries {
        let i = index as usize;
        if i == 0 || i >= SLOTS - 1 {
            continue;
        }
        t[i * 4..i * 4 + 4].copy_from_slice(&action.encode());
    }
    t[TABLE_LEN - 2..].copy_from_slice(&BLOCK_TRAILER);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodings_match_original() {
        assert_eq!(KeyAction::Default.encode(), [0, 0, 0, 0]);
        assert_eq!(KeyAction::Disabled.encode(), [5, 3, 0, 0]);
        assert_eq!(KeyAction::parse("key:Win+D").unwrap().encode(), [2, 0x08, 0x07, 0]);
        assert_eq!(KeyAction::parse("key:Ctrl+C").unwrap().encode(), [2, 0x01, 0x06, 0]);
        assert_eq!(KeyAction::parse("key:Alt+Tab").unwrap().encode(), [2, 0x04, 0x2B, 0]);
        assert_eq!(KeyAction::parse("key:LShift").unwrap().encode(), [2, 0x02, 0, 0]);
        assert_eq!(KeyAction::parse("key:A").unwrap().encode(), [2, 0, 0x04, 0]);
        assert_eq!(KeyAction::parse("mouse:left-click").unwrap().encode(), [1, 1, 1, 0]);
        assert_eq!(KeyAction::parse("mouse:middle-click").unwrap().encode(), [1, 1, 4, 0]);
        assert_eq!(KeyAction::parse("mouse:wheel-down").unwrap().encode(), [1, 3, 0xFF, 0]);
        assert_eq!(KeyAction::parse("media:mute").unwrap().encode(), [3, 0xE2, 0, 0]);
        assert_eq!(KeyAction::parse("media:calculator").unwrap().encode(), [3, 0x92, 0x01, 0]);
        assert_eq!(KeyAction::parse("macro:1").unwrap().encode(), [6, 0, 0, 1]);
        assert_eq!(KeyAction::parse("key:F13").unwrap().encode(), [2, 0, 0x68, 0]);
    }

    #[test]
    fn text_round_trip() {
        for s in ["default", "disabled", "key:Ctrl+Shift+Esc", "key:A", "key:RShift", "mouse:back", "media:volume-up", "macro:7"] {
            let a = KeyAction::parse(s).unwrap();
            assert_eq!(KeyAction::parse(&a.to_text()).unwrap(), a, "{s}");
        }
        assert_eq!(KeyAction::parse("key:ctrl+c").unwrap().to_text(), "key:Ctrl+C");
    }

    #[test]
    fn parse_errors_are_clear() {
        assert!(KeyAction::parse("key:").unwrap_err().contains("give a key"));
        assert!(KeyAction::parse("key:Foo").unwrap_err().contains("unknown key"));
        assert!(KeyAction::parse("key:A+B").unwrap_err().contains("not a modifier"));
        assert!(KeyAction::parse("key:Fn").unwrap_err().contains("unknown key"));
        assert!(KeyAction::parse("mouse:x").unwrap_err().contains("left-click"));
        assert!(KeyAction::parse("macro:0").is_err());
        assert!(KeyAction::parse("jump").is_err());
    }

    #[test]
    fn table_layout() {
        let t = table(&[(1, KeyAction::Disabled), (0, KeyAction::Disabled)]);
        assert_eq!(&t[4..8], &[5, 3, 0, 0]);
        assert_eq!(&t[0..4], &[0, 0, 0, 0]);
        assert_eq!(&t[TABLE_LEN - 2..], &[0xAA, 0x55]);
    }
}
