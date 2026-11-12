//! Physical layout of the M87 Pro (87 keys, ANSI).
//!
//! Generated from the vendor layout file (`layouts/rgb-keyboard.xml`).
//! `index` is the key's slot in the keyboard's key-map and LED tables
//! (the vendor file uses the same number for both). Positions are in
//! quarter-key units: a normal key is `w4 = 4` wide.

/// One physical key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    /// Stable identifier used in the CLI and saved files.
    pub id: &'static str,
    /// Short text printed on the on-screen keyboard.
    pub label: &'static str,
    /// HID keyboard usage the key sends by default (0xAF = Fn).
    pub usage: u8,
    /// Slot in the key-map and LED tables (1..=143).
    pub index: u8,
    /// Row from the top (0 = function row).
    pub row: u8,
    /// Left edge, in quarter-key units.
    pub x4: u8,
    /// Width, in quarter-key units.
    pub w4: u8,
}

/// Number of slots in the key-map and LED tables.
pub const SLOTS: usize = 144;

#[rustfmt::skip]
pub const KEYS: &[Key] = &[
    Key { id: "Esc", label: "Esc", usage: 0x29, index: 1, row: 0, x4: 0, w4: 4 },
    Key { id: "F1", label: "F1", usage: 0x3A, index: 2, row: 0, x4: 8, w4: 4 },
    Key { id: "F2", label: "F2", usage: 0x3B, index: 3, row: 0, x4: 12, w4: 4 },
    Key { id: "F3", label: "F3", usage: 0x3C, index: 4, row: 0, x4: 16, w4: 4 },
    Key { id: "F4", label: "F4", usage: 0x3D, index: 5, row: 0, x4: 20, w4: 4 },
    Key { id: "F5", label: "F5", usage: 0x3E, index: 6, row: 0, x4: 26, w4: 4 },
    Key { id: "F6", label: "F6", usage: 0x3F, index: 7, row: 0, x4: 30, w4: 4 },
    Key { id: "F7", label: "F7", usage: 0x40, index: 8, row: 0, x4: 34, w4: 4 },
    Key { id: "F8", label: "F8", usage: 0x41, index: 9, row: 0, x4: 38, w4: 4 },
    Key { id: "F9", label: "F9", usage: 0x42, index: 10, row: 0, x4: 44, w4: 4 },
    Key { id: "F10", label: "F10", usage: 0x43, index: 11, row: 0, x4: 48, w4: 4 },
    Key { id: "F11", label: "F11", usage: 0x44, index: 12, row: 0, x4: 52, w4: 4 },
    Key { id: "F12", label: "F12", usage: 0x45, index: 13, row: 0, x4: 56, w4: 4 },
    Key { id: "PrintScreen", label: "PrtSc", usage: 0x46, index: 112, row: 0, x4: 61, w4: 4 },
    Key { id: "ScrollLock", label: "ScrLk", usage: 0x47, index: 113, row: 0, x4: 65, w4: 4 },
    Key { id: "Pause", label: "Pause", usage: 0x48, index: 115, row: 0, x4: 69, w4: 4 },
    Key { id: "Grave", label: "`", usage: 0x35, index: 19, row: 1, x4: 0, w4: 4 },
    Key { id: "Digit1", label: "1", usage: 0x1E, index: 20, row: 1, x4: 4, w4: 4 },
    Key { id: "Digit2", label: "2", usage: 0x1F, index: 21, row: 1, x4: 8, w4: 4 },
    Key { id: "Digit3", label: "3", usage: 0x20, index: 22, row: 1, x4: 12, w4: 4 },
    Key { id: "Digit4", label: "4", usage: 0x21, index: 23, row: 1, x4: 16, w4: 4 },
    Key { id: "Digit5", label: "5", usage: 0x22, index: 24, row: 1, x4: 20, w4: 4 },
    Key { id: "Digit6", label: "6", usage: 0x23, index: 25, row: 1, x4: 24, w4: 4 },
    Key { id: "Digit7", label: "7", usage: 0x24, index: 26, row: 1, x4: 28, w4: 4 },
    Key { id: "Digit8", label: "8", usage: 0x25, index: 27, row: 1, x4: 32, w4: 4 },
    Key { id: "Digit9", label: "9", usage: 0x26, index: 28, row: 1, x4: 36, w4: 4 },
    Key { id: "Digit0", label: "0", usage: 0x27, index: 29, row: 1, x4: 40, w4: 4 },
    Key { id: "Minus", label: "-", usage: 0x2D, index: 30, row: 1, x4: 44, w4: 4 },
    Key { id: "Equal", label: "=", usage: 0x2E, index: 31, row: 1, x4: 48, w4: 4 },
    Key { id: "Backspace", label: "Backspace", usage: 0x2A, index: 103, row: 1, x4: 52, w4: 8 },
    Key { id: "Insert", label: "Ins", usage: 0x49, index: 116, row: 1, x4: 61, w4: 4 },
    Key { id: "Home", label: "Home", usage: 0x4A, index: 117, row: 1, x4: 65, w4: 4 },
    Key { id: "PgUp", label: "PgUp", usage: 0x4B, index: 118, row: 1, x4: 69, w4: 4 },
    Key { id: "Tab", label: "Tab", usage: 0x2B, index: 37, row: 2, x4: 0, w4: 6 },
    Key { id: "Q", label: "Q", usage: 0x14, index: 38, row: 2, x4: 6, w4: 4 },
    Key { id: "W", label: "W", usage: 0x1A, index: 39, row: 2, x4: 10, w4: 4 },
    Key { id: "E", label: "E", usage: 0x08, index: 40, row: 2, x4: 14, w4: 4 },
    Key { id: "R", label: "R", usage: 0x15, index: 41, row: 2, x4: 18, w4: 4 },
    Key { id: "T", label: "T", usage: 0x17, index: 42, row: 2, x4: 22, w4: 4 },
    Key { id: "Y", label: "Y", usage: 0x1C, index: 43, row: 2, x4: 26, w4: 4 },
    Key { id: "U", label: "U", usage: 0x18, index: 44, row: 2, x4: 30, w4: 4 },
    Key { id: "I", label: "I", usage: 0x0C, index: 45, row: 2, x4: 34, w4: 4 },
    Key { id: "O", label: "O", usage: 0x12, index: 46, row: 2, x4: 38, w4: 4 },
    Key { id: "P", label: "P", usage: 0x13, index: 47, row: 2, x4: 42, w4: 4 },
    Key { id: "LBracket", label: "[", usage: 0x2F, index: 48, row: 2, x4: 46, w4: 4 },
    Key { id: "RBracket", label: "]", usage: 0x30, index: 49, row: 2, x4: 50, w4: 4 },
    Key { id: "Backslash", label: "\\", usage: 0x31, index: 67, row: 2, x4: 54, w4: 6 },
    Key { id: "Delete", label: "Del", usage: 0x4C, index: 119, row: 2, x4: 61, w4: 4 },
    Key { id: "End", label: "End", usage: 0x4D, index: 120, row: 2, x4: 65, w4: 4 },
    Key { id: "PgDn", label: "PgDn", usage: 0x4E, index: 121, row: 2, x4: 69, w4: 4 },
    Key { id: "CapsLock", label: "Caps Lock", usage: 0x39, index: 55, row: 3, x4: 0, w4: 7 },
    Key { id: "A", label: "A", usage: 0x04, index: 56, row: 3, x4: 7, w4: 4 },
    Key { id: "S", label: "S", usage: 0x16, index: 57, row: 3, x4: 11, w4: 4 },
    Key { id: "D", label: "D", usage: 0x07, index: 58, row: 3, x4: 15, w4: 4 },
    Key { id: "F", label: "F", usage: 0x09, index: 59, row: 3, x4: 19, w4: 4 },
    Key { id: "G", label: "G", usage: 0x0A, index: 60, row: 3, x4: 23, w4: 4 },
    Key { id: "H", label: "H", usage: 0x0B, index: 61, row: 3, x4: 27, w4: 4 },
    Key { id: "J", label: "J", usage: 0x0D, index: 62, row: 3, x4: 31, w4: 4 },
    Key { id: "K", label: "K", usage: 0x0E, index: 63, row: 3, x4: 35, w4: 4 },
    Key { id: "L", label: "L", usage: 0x0F, index: 64, row: 3, x4: 39, w4: 4 },
    Key { id: "Semicolon", label: ";", usage: 0x33, index: 65, row: 3, x4: 43, w4: 4 },
    Key { id: "Quote", label: "'", usage: 0x34, index: 66, row: 3, x4: 47, w4: 4 },
    Key { id: "Enter", label: "Enter", usage: 0x28, index: 85, row: 3, x4: 51, w4: 9 },
    Key { id: "LShift", label: "Shift", usage: 0xE1, index: 73, row: 4, x4: 0, w4: 9 },
    Key { id: "Z", label: "Z", usage: 0x1D, index: 74, row: 4, x4: 9, w4: 4 },
    Key { id: "X", label: "X", usage: 0x1B, index: 75, row: 4, x4: 13, w4: 4 },
    Key { id: "C", label: "C", usage: 0x06, index: 76, row: 4, x4: 17, w4: 4 },
    Key { id: "V", label: "V", usage: 0x19, index: 77, row: 4, x4: 21, w4: 4 },
    Key { id: "B", label: "B", usage: 0x05, index: 78, row: 4, x4: 25, w4: 4 },
    Key { id: "N", label: "N", usage: 0x11, index: 79, row: 4, x4: 29, w4: 4 },
    Key { id: "M", label: "M", usage: 0x10, index: 80, row: 4, x4: 33, w4: 4 },
    Key { id: "Comma", label: ",", usage: 0x36, index: 81, row: 4, x4: 37, w4: 4 },
    Key { id: "Period", label: ".", usage: 0x37, index: 82, row: 4, x4: 41, w4: 4 },
    Key { id: "Slash", label: "/", usage: 0x38, index: 83, row: 4, x4: 45, w4: 4 },
    Key { id: "RShift", label: "Shift", usage: 0xE5, index: 84, row: 4, x4: 49, w4: 11 },
    Key { id: "Up", label: "Up", usage: 0x52, index: 101, row: 4, x4: 65, w4: 4 },
    Key { id: "LCtrl", label: "Ctrl", usage: 0xE0, index: 91, row: 5, x4: 0, w4: 5 },
    Key { id: "LWin", label: "Win", usage: 0xE3, index: 92, row: 5, x4: 5, w4: 5 },
    Key { id: "LAlt", label: "Alt", usage: 0xE2, index: 93, row: 5, x4: 10, w4: 5 },
    Key { id: "Space", label: "Space", usage: 0x2C, index: 94, row: 5, x4: 15, w4: 25 },
    Key { id: "RAlt", label: "Alt", usage: 0xE6, index: 95, row: 5, x4: 40, w4: 5 },
    Key { id: "Fn", label: "Fn", usage: 0xAF, index: 96, row: 5, x4: 45, w4: 5 },
    Key { id: "Menu", label: "Menu", usage: 0x65, index: 97, row: 5, x4: 50, w4: 5 },
    Key { id: "RCtrl", label: "Ctrl", usage: 0xE4, index: 98, row: 5, x4: 55, w4: 5 },
    Key { id: "Left", label: "Left", usage: 0x50, index: 99, row: 5, x4: 61, w4: 4 },
    Key { id: "Down", label: "Down", usage: 0x51, index: 100, row: 5, x4: 65, w4: 4 },
    Key { id: "Right", label: "Right", usage: 0x4F, index: 102, row: 5, x4: 69, w4: 4 },
];

/// Find a key by its id, ignoring case (for example `esc`, `LShift`, `Digit1`).
pub fn find(id: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.id.eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_consistent() {
        assert_eq!(KEYS.len(), 87);
        let mut seen = [false; SLOTS];
        for k in KEYS {
            assert!((k.index as usize) < SLOTS && k.index > 0, "{}", k.id);
            assert!(!seen[k.index as usize], "duplicate index {}", k.index);
            seen[k.index as usize] = true;
            assert!(k.row < 6);
        }
        let mut ids: Vec<_> = KEYS.iter().map(|k| k.id.to_ascii_lowercase()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 87, "key ids must be unique");
    }

    #[test]
    fn lookup() {
        assert_eq!(find("esc").unwrap().index, 1);
        assert_eq!(find("SPACE").unwrap().usage, 0x2C);
        assert!(find("nope").is_none());
    }
}
