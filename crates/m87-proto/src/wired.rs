//! USB-cable transport: 65-byte feature reports (report ID 0 + 64 data bytes).
//!
//! A write is a sequence of packets:
//! `04 18` (begin) → `04 CC 00 00 00 00 00 00 NN` (header) → NN × 64-byte data
//! blocks ending in `AA 55` → `04 02` (apply). Some commands then send `04 F0`.

/// Bytes per feature report, including the report ID.
pub const REPORT_LEN: usize = 65;
/// Data bytes per packet.
pub const DATA_LEN: usize = 64;

/// Sub-command bytes (data[1]) after the `04` class byte.
pub mod cc {
    pub const APPLY: u8 = 0x02;
    pub const BEGIN: u8 = 0x18;
    pub const BEGIN_MACRO: u8 = 0x19;
    pub const KEYMAP_BASE: u8 = 0x11;
    pub const LIGHTING: u8 = 0x13;
    pub const MACROS: u8 = 0x15;
    pub const SETTINGS: u8 = 0x17;
    pub const LIVE_FRAME: u8 = 0x20;
    pub const USER_LIGHT: u8 = 0x23;
    pub const KEYMAP_FN: u8 = 0x27;
    pub const FINISH: u8 = 0xF0;
    pub const READ_LEDS: u8 = 0xF5;
}

/// Control packet `04 CC … NN` (NN at data[8] = number of data packets that follow).
pub fn control(cc: u8, following: u8) -> [u8; REPORT_LEN] {
    let mut r = [0u8; REPORT_LEN];
    r[1] = 0x04;
    r[2] = cc;
    r[1 + 8] = following;
    r
}

/// Begin packet `04 18`.
pub fn begin() -> [u8; REPORT_LEN] {
    control(cc::BEGIN, 0)
}

/// Apply packet `04 02`.
pub fn apply() -> [u8; REPORT_LEN] {
    control(cc::APPLY, 0)
}

/// Split a data block into feature reports. The block is zero-padded to a
/// multiple of 64 bytes; the caller is responsible for the `AA 55` trailer.
pub fn data_packets(block: &[u8]) -> Vec<[u8; REPORT_LEN]> {
    block
        .chunks(DATA_LEN)
        .map(|chunk| {
            let mut r = [0u8; REPORT_LEN];
            r[1..1 + chunk.len()].copy_from_slice(chunk);
            r
        })
        .collect()
}

/// True when a feature read after a control packet acknowledges it: the
/// reply repeats data[0..2] and sets data[3] = 1 (observed on the device).
pub fn is_ack(request: &[u8; REPORT_LEN], reply: &[u8]) -> bool {
    reply.len() >= 5 && reply[1] == request[1] && reply[2] == request[2] && reply[4] == 0x01
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ack_check() {
        let req = control(cc::LIGHTING, 1);
        let mut ok = req;
        ok[4] = 1;
        assert!(is_ack(&req, &ok));
        assert!(!is_ack(&req, &req));
        let mut other = ok;
        other[2] = cc::APPLY;
        assert!(!is_ack(&req, &other));
    }

    #[test]
    fn control_layout() {
        let r = control(cc::LIGHTING, 1);
        assert_eq!(r.len(), 65);
        assert_eq!(&r[..10], &[0x00, 0x04, 0x13, 0, 0, 0, 0, 0, 0, 0x01]);
        assert!(r[10..].iter().all(|b| *b == 0));
    }

    #[test]
    fn begin_and_apply() {
        assert_eq!(&begin()[..3], &[0x00, 0x04, 0x18]);
        assert_eq!(&apply()[..3], &[0x00, 0x04, 0x02]);
    }

    #[test]
    fn split_blocks() {
        let block: Vec<u8> = (0..130u8).collect();
        let p = data_packets(&block);
        assert_eq!(p.len(), 3);
        assert_eq!(p[0][0], 0);
        assert_eq!(p[0][1], 0);
        assert_eq!(p[1][1], 64);
        assert_eq!(p[2][1], 128);
        assert_eq!(p[2][3], 0);
    }
}
