//! Lighting effects and per-key colors.

use crate::layout::SLOTS;
use crate::BLOCK_TRAILER;

/// Built-in lighting effects. Numbers are the firmware's mode IDs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Effect {
    Off = 0,
    Static = 1,
    SingleOn = 2,
    SingleOff = 3,
    Glittering = 4,
    Falling = 5,
    Colourful = 6,
    Breath = 7,
    Spectrum = 8,
    Outward = 9,
    Scrolling = 10,
    Rolling = 11,
    Rotating = 12,
    Explode = 13,
    Launch = 14,
    Ripples = 15,
    Flowing = 16,
    Pulsating = 17,
    Tilt = 18,
    Shuttle = 19,
}

/// Which settings an effect uses (from the original app's capability mask).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Controls {
    pub brightness: bool,
    pub speed: bool,
    pub direction: bool,
    pub color: bool,
}

impl Effect {
    pub const ALL: [Effect; 20] = [
        Effect::Off,
        Effect::Static,
        Effect::SingleOn,
        Effect::SingleOff,
        Effect::Glittering,
        Effect::Falling,
        Effect::Colourful,
        Effect::Breath,
        Effect::Spectrum,
        Effect::Outward,
        Effect::Scrolling,
        Effect::Rolling,
        Effect::Rotating,
        Effect::Explode,
        Effect::Launch,
        Effect::Ripples,
        Effect::Flowing,
        Effect::Pulsating,
        Effect::Tilt,
        Effect::Shuttle,
    ];

    pub fn id(self) -> u8 {
        self as u8
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Effect::ALL.get(id as usize).copied()
    }

    /// Name used in the CLI and saved files (lowercase, no spaces).
    pub fn name(self) -> &'static str {
        match self {
            Effect::Off => "off",
            Effect::Static => "static",
            Effect::SingleOn => "single-on",
            Effect::SingleOff => "single-off",
            Effect::Glittering => "glittering",
            Effect::Falling => "falling",
            Effect::Colourful => "colorful",
            Effect::Breath => "breath",
            Effect::Spectrum => "spectrum",
            Effect::Outward => "outward",
            Effect::Scrolling => "scrolling",
            Effect::Rolling => "rolling",
            Effect::Rotating => "rotating",
            Effect::Explode => "explode",
            Effect::Launch => "launch",
            Effect::Ripples => "ripples",
            Effect::Flowing => "flowing",
            Effect::Pulsating => "pulsating",
            Effect::Tilt => "tilt",
            Effect::Shuttle => "shuttle",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        let n = name.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        let n = if n == "colourful" { "colorful".to_string() } else { n };
        Effect::ALL.into_iter().find(|e| e.name() == n)
    }

    /// Settings this effect reacts to. Decoded from the original app's
    /// `config_func` mask (bit 0 brightness, 1 speed, 2 direction, 4 color).
    pub fn controls(self) -> Controls {
        let mask: u8 = match self {
            Effect::Off => 0x00,
            Effect::Static => 0x31,
            Effect::Colourful | Effect::Spectrum => 0x03,
            Effect::Scrolling => 0x3B,
            Effect::Rolling | Effect::Rotating | Effect::Flowing | Effect::Tilt => 0x37,
            _ => 0x33,
        };
        Controls {
            brightness: mask & 0x01 != 0,
            speed: mask & 0x02 != 0,
            direction: mask & 0x0C != 0,
            color: mask & 0x10 != 0,
        }
    }
}

/// 24-bit color.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }

    /// Parse `RRGGBB`, `#RRGGBB`, or a basic color name.
    pub fn parse(s: &str) -> Option<Self> {
        let t = s.trim().trim_start_matches('#').to_ascii_lowercase();
        let named = match t.as_str() {
            "red" => Some(0xFF0000),
            "orange" => Some(0xFF8000),
            "yellow" => Some(0xFFFF00),
            "green" => Some(0x00FF00),
            "cyan" => Some(0x00FFFF),
            "blue" => Some(0x0000FF),
            "purple" => Some(0x8000FF),
            "white" => Some(0xFFFFFF),
            "black" | "off" => Some(0x000000),
            _ => None,
        };
        let v = match named {
            Some(v) => v,
            None if t.len() == 6 => u32::from_str_radix(&t, 16).ok()?,
            None => return None,
        };
        Some(Rgb::new((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }

    pub fn to_hex(self) -> String {
        format!("{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }
}

/// Highest brightness and speed level.
pub const MAX_LEVEL: u8 = 5;

/// Settings for a built-in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lighting {
    pub effect: Effect,
    pub color: Rgb,
    /// Cycle through all colors instead of using `color`.
    pub rainbow: bool,
    /// 0..=5
    pub brightness: u8,
    /// 0..=5
    pub speed: u8,
    /// Raw direction value; meaning depends on the effect.
    pub direction: u8,
}

impl Default for Lighting {
    /// The keyboard's factory look: Rolling, rainbow, brightness 5, speed 3.
    fn default() -> Self {
        Lighting {
            effect: Effect::Rolling,
            color: Rgb::new(0xFF, 0, 0),
            rainbow: true,
            brightness: MAX_LEVEL,
            speed: 3,
            direction: 0,
        }
    }
}

/// Mode ID that switches the keyboard to per-key colors.
pub const MODE_PER_KEY: u8 = 0x80;

/// 16-byte lighting block (identical on both connections):
/// `mode R G B 0 0 0 0 rainbow brightness speed direction 0 0 AA 55`.
pub fn effect_block(l: &Lighting) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0] = l.effect.id();
    if l.effect != Effect::Off {
        b[1] = l.color.r;
        b[2] = l.color.g;
        b[3] = l.color.b;
        b[8] = l.rainbow as u8;
        b[9] = l.brightness.min(MAX_LEVEL);
        b[10] = l.speed.min(MAX_LEVEL);
        b[11] = l.direction;
    }
    b[14..16].copy_from_slice(&BLOCK_TRAILER);
    b
}

/// 16-byte block that switches to per-key colors at the given brightness.
pub fn per_key_block(brightness: u8) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[0] = MODE_PER_KEY;
    b[9] = brightness.min(MAX_LEVEL);
    b[14..16].copy_from_slice(&BLOCK_TRAILER);
    b
}

/// Size of the per-key color table.
pub const COLOR_TABLE_LEN: usize = SLOTS * 4;

/// Per-key color table: entry `index*4` = `[index, R, G, B]`, `AA 55` at the
/// end. Keys that are not listed stay dark. Index 0 is never written.
pub fn color_table(colors: &[(u8, Rgb)]) -> [u8; COLOR_TABLE_LEN] {
    let mut t = [0u8; COLOR_TABLE_LEN];
    for &(index, c) in colors {
        let i = index as usize;
        if i == 0 || i >= SLOTS - 1 {
            continue;
        }
        t[i * 4] = index;
        t[i * 4 + 1] = c.r;
        t[i * 4 + 2] = c.g;
        t[i * 4 + 3] = c.b;
    }
    t[COLOR_TABLE_LEN - 2..].copy_from_slice(&BLOCK_TRAILER);
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_ids_and_names() {
        for (i, e) in Effect::ALL.iter().enumerate() {
            assert_eq!(e.id() as usize, i);
            assert_eq!(Effect::from_id(i as u8), Some(*e));
            assert_eq!(Effect::from_name(e.name()), Some(*e));
        }
        assert_eq!(Effect::from_name("Colourful"), Some(Effect::Colourful));
        assert_eq!(Effect::from_name("single_on"), Some(Effect::SingleOn));
        assert_eq!(Effect::from_id(20), None);
    }

    #[test]
    fn controls_mask() {
        let s = Effect::Static.controls();
        assert!(s.brightness && s.color && !s.speed && !s.direction);
        let r = Effect::Rolling.controls();
        assert!(r.brightness && r.speed && r.direction && r.color);
        assert_eq!(Effect::Off.controls(), Controls { brightness: false, speed: false, direction: false, color: false });
    }

    #[test]
    fn rgb_parse() {
        assert_eq!(Rgb::parse("#FF8000"), Some(Rgb::new(255, 128, 0)));
        assert_eq!(Rgb::parse("00ff00"), Some(Rgb::new(0, 255, 0)));
        assert_eq!(Rgb::parse("Blue"), Some(Rgb::new(0, 0, 255)));
        assert_eq!(Rgb::parse("12345"), None);
        assert_eq!(Rgb::parse("zzzzzz"), None);
        assert_eq!(Rgb::new(1, 2, 3).to_hex(), "010203");
    }

    #[test]
    fn effect_block_layout() {
        let l = Lighting {
            effect: Effect::Breath,
            color: Rgb::new(0x11, 0x22, 0x33),
            rainbow: false,
            brightness: 9,
            speed: 2,
            direction: 1,
        };
        assert_eq!(effect_block(&l), [7, 0x11, 0x22, 0x33, 0, 0, 0, 0, 0, 5, 2, 1, 0, 0, 0xAA, 0x55]);
    }

    #[test]
    fn off_block_has_no_settings() {
        let l = Lighting { effect: Effect::Off, ..Lighting::default() };
        assert_eq!(effect_block(&l), [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xAA, 0x55]);
    }

    #[test]
    fn per_key_block_layout() {
        assert_eq!(per_key_block(4), [0x80, 0, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 0, 0xAA, 0x55]);
    }

    #[test]
    fn color_table_layout() {
        let t = color_table(&[(1, Rgb::new(1, 2, 3)), (0, Rgb::new(9, 9, 9)), (143, Rgb::new(9, 9, 9))]);
        assert_eq!(&t[4..8], &[1, 1, 2, 3]);
        assert_eq!(&t[0..4], &[0, 0, 0, 0]);
        assert_eq!(&t[COLOR_TABLE_LEN - 4..], &[0, 0, 0xAA, 0x55]);
    }
}
