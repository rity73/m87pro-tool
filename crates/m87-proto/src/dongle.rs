//! 2.4 GHz receiver transport: 33-byte output reports.
//!
//! ```text
//! byte 0      0x00   report ID
//! byte 1      CMD    command
//! byte 2      LEN    payload length (max 28)
//! byte 3      IDX    packet index / sub-index
//! byte 4..31         payload
//! byte 32     CHK    8-bit sum of bytes 0..=31
//! ```
//! The receiver acknowledges with an input report whose first three data
//! bytes repeat CMD, LEN and IDX (the report ID is not part of the reply).

/// Bytes written per report, including the report ID.
pub const REPORT_LEN: usize = 33;
/// Data bytes in a reply (the report ID is stripped by the HID layer).
pub const REPLY_LEN: usize = 32;
/// Largest payload that fits in one report.
pub const MAX_PAYLOAD: usize = 28;

/// Command bytes (byte 1). Only the ones used so far are listed.
pub mod cmd {
    /// Is the keyboard reachable through the receiver?
    pub const PING: u8 = 0x02;
    /// Lighting block (effect, or switch to per-key colors).
    pub const LIGHTING: u8 = 0x05;
    /// System settings block.
    pub const SETTINGS: u8 = 0x07;
    /// Factory reset of all keyboard data.
    pub const FACTORY_RESET: u8 = 0x0F;
    /// One 28-byte slice of the base-layer key map.
    pub const KEYMAP_BASE: u8 = 0x10;
    /// One 28-byte slice of the Fn-layer key map.
    pub const KEYMAP_FN: u8 = 0x12;
    /// One 28-byte slice of the per-key color table.
    pub const KEY_COLORS: u8 = 0x14;
    /// Return all keys to their factory functions.
    pub const RESET_KEYS: u8 = 0x1F;
    /// Read the battery level.
    pub const BATTERY: u8 = 0x20;
}

/// Errors when building a report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameError {
    PayloadTooLong(usize),
}

impl std::fmt::Display for FrameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FrameError::PayloadTooLong(n) => {
                write!(f, "payload is {n} bytes; the limit is {MAX_PAYLOAD}")
            }
        }
    }
}

impl std::error::Error for FrameError {}

/// Sum of bytes 0..=31, as the receiver expects in byte 32.
pub fn checksum(report: &[u8; REPORT_LEN]) -> u8 {
    report[..32].iter().fold(0u8, |acc, b| acc.wrapping_add(*b))
}

/// Build one report. `len` is written to byte 2 as-is because some commands
/// send a LEN that differs from the payload size (for example battery uses 1).
pub fn frame(cmd: u8, len: u8, idx: u8, payload: &[u8]) -> Result<[u8; REPORT_LEN], FrameError> {
    if payload.len() > MAX_PAYLOAD {
        return Err(FrameError::PayloadTooLong(payload.len()));
    }
    let mut r = [0u8; REPORT_LEN];
    r[1] = cmd;
    r[2] = len;
    r[3] = idx;
    r[4..4 + payload.len()].copy_from_slice(payload);
    r[32] = checksum(&r);
    Ok(r)
}

/// True when `reply` acknowledges `request`.
pub fn is_ack(request: &[u8; REPORT_LEN], reply: &[u8]) -> bool {
    reply.len() >= 3 && reply[0] == request[1] && reply[1] == request[2] && reply[2] == request[3]
}

/// Ping report: `00 02 00 00 … CHK`.
pub fn ping() -> [u8; REPORT_LEN] {
    frame(cmd::PING, 0, 0, &[]).expect("empty payload")
}

/// Battery query report: `00 20 01 00 … CHK`.
pub fn battery_query() -> [u8; REPORT_LEN] {
    frame(cmd::BATTERY, 1, 0, &[]).expect("empty payload")
}

/// Battery level in percent from an acknowledged battery reply (byte 3).
/// Values above 100 are clamped, as the original app does.
pub fn parse_battery(reply: &[u8]) -> Option<u8> {
    if reply.len() < 4 || reply[0] != cmd::BATTERY {
        return None;
    }
    Some(reply[3].min(100))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ping_layout() {
        let r = ping();
        assert_eq!(r.len(), 33);
        assert_eq!(&r[..4], &[0x00, 0x02, 0x00, 0x00]);
        assert!(r[4..32].iter().all(|b| *b == 0));
        assert_eq!(r[32], 0x02);
    }

    #[test]
    fn battery_layout() {
        let r = battery_query();
        assert_eq!(&r[..4], &[0x00, 0x20, 0x01, 0x00]);
        assert_eq!(r[32], 0x21);
    }

    #[test]
    fn checksum_wraps() {
        let r = frame(0xF0, 28, 0xFF, &[0xFF; 28]).unwrap();
        let expected = (0xF0u32 + 28 + 0xFF + 0xFF * 28) as u8;
        assert_eq!(r[32], expected);
    }

    #[test]
    fn payload_limit() {
        assert!(frame(1, 0, 0, &[0; 28]).is_ok());
        assert_eq!(frame(1, 0, 0, &[0; 29]), Err(FrameError::PayloadTooLong(29)));
    }

    #[test]
    fn ack_matching() {
        let req = battery_query();
        assert!(is_ack(&req, &[0x20, 0x01, 0x00, 0x57]));
        assert!(!is_ack(&req, &[0x20, 0x00, 0x00, 0x57]));
        assert!(!is_ack(&req, &[0x20, 0x01]));
    }

    #[test]
    fn battery_parse() {
        assert_eq!(parse_battery(&[0x20, 0x01, 0x00, 87]), Some(87));
        assert_eq!(parse_battery(&[0x20, 0x01, 0x00, 150]), Some(100));
        assert_eq!(parse_battery(&[0x02, 0x01, 0x00, 87]), None);
        assert_eq!(parse_battery(&[0x20, 0x01, 0x00]), None);
    }
}
