//! Keyboard system settings block (`docs/protocol.md` section 2.1).

use crate::BLOCK_TRAILER;

/// Settings the keyboard stores. The keyboard cannot report them, so the
/// tool keeps the last values it sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// Block Alt+Tab.
    pub disable_alt_tab: bool,
    /// Block Alt+F4.
    pub disable_alt_f4: bool,
    /// Block the Windows key.
    pub disable_win: bool,
    /// Raw `fn_switch` value; meaning still to be confirmed on the device.
    pub fn_switch: u8,
    /// Raw `sleep_time` value. Believed: 0 never, 1 = 1 min, 2 = 5 min, 3 = 30 min.
    pub sleep: u8,
    /// Raw `key_respondtime` (debounce) value. Believed: level 1..5.
    pub response: u8,
}

impl Default for Settings {
    /// The original app's defaults.
    fn default() -> Self {
        Settings { disable_alt_tab: false, disable_alt_f4: false, disable_win: false, fn_switch: 0, sleep: 2, response: 2 }
    }
}

/// Sleep timer choices shown in the interface, with their raw values.
pub const SLEEP_CHOICES: [(&str, u8); 4] = [("never", 0), ("1min", 1), ("5min", 2), ("30min", 3)];

/// 16 meaningful bytes of the settings block; the wired block puts the
/// trailer at the end of 64 bytes instead of at byte 14.
fn fields(s: &Settings) -> [u8; 9] {
    [0x00, 0x01, s.disable_alt_tab as u8, s.disable_alt_f4 as u8, s.disable_win as u8, s.fn_switch, s.sleep, 0x00, s.response]
}

/// 16-byte block for the receiver (CMD 0x07), `AA 55` at 14.
pub fn dongle_block(s: &Settings) -> [u8; 16] {
    let mut b = [0u8; 16];
    b[..9].copy_from_slice(&fields(s));
    b[14..].copy_from_slice(&BLOCK_TRAILER);
    b
}

/// 64-byte block for the USB cable (`04 17`), `AA 55` at 62.
pub fn wired_block(s: &Settings) -> [u8; 64] {
    let mut b = [0u8; 64];
    b[..9].copy_from_slice(&fields(s));
    b[62..].copy_from_slice(&BLOCK_TRAILER);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks() {
        let s = Settings { disable_alt_tab: true, disable_alt_f4: false, disable_win: true, fn_switch: 1, sleep: 3, response: 4 };
        assert_eq!(dongle_block(&s), [0, 1, 1, 0, 1, 1, 3, 0, 4, 0, 0, 0, 0, 0, 0xAA, 0x55]);
        let w = wired_block(&s);
        assert_eq!(&w[..9], &[0, 1, 1, 0, 1, 1, 3, 0, 4]);
        assert_eq!(&w[62..], &[0xAA, 0x55]);
        assert!(w[9..62].iter().all(|b| *b == 0));
    }
}
