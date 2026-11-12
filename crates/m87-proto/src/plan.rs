//! Complete packet sequences for each operation, per connection type.
//!
//! The transport layer just executes a `Vec<Packet>`, and the CLI can print
//! the same list for `--dry-run`.

use crate::lighting::{color_table, effect_block, per_key_block, Lighting, Rgb, COLOR_TABLE_LEN};
use crate::{dongle, wired, Link};

/// How one packet is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Kind {
    /// Receiver: 33-byte output report, then wait for the echo reply.
    DongleAck,
    /// USB cable: 65-byte feature report; `read_back` adds a feature read
    /// (a handshake; the content is not checked).
    Feature { read_back: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Packet {
    pub kind: Kind,
    pub bytes: Vec<u8>,
    /// Extra pause after this packet, in milliseconds.
    pub pause_ms: u32,
}

impl Packet {
    fn dongle(bytes: [u8; dongle::REPORT_LEN], pause_ms: u32) -> Self {
        Packet { kind: Kind::DongleAck, bytes: bytes.to_vec(), pause_ms }
    }
    fn feature(bytes: [u8; wired::REPORT_LEN], read_back: bool) -> Self {
        Packet { kind: Kind::Feature { read_back }, bytes: bytes.to_vec(), pause_ms: 0 }
    }
}

/// Wired: `begin` → `04 CC … 1` → one data packet → `apply` → `04 F0`.
fn wired_single_block(cc: u8, block: &[u8]) -> Vec<Packet> {
    let mut data = [0u8; wired::DATA_LEN];
    data[..block.len()].copy_from_slice(block);
    vec![
        Packet::feature(wired::begin(), true),
        Packet::feature(wired::control(cc, 1), true),
        Packet::feature(wired::data_packets(&data)[0], false),
        Packet::feature(wired::apply(), true),
        Packet::feature(wired::control(wired::cc::FINISH, 0), false),
    ]
}

/// Switch to a built-in effect.
pub fn set_effect(link: Link, l: &Lighting) -> Vec<Packet> {
    let block = effect_block(l);
    match link {
        Link::Dongle => vec![Packet::dongle(dongle::frame(dongle::cmd::LIGHTING, 0x10, 0, &block).unwrap(), 0)],
        Link::Wired => wired_single_block(wired::cc::LIGHTING, &block),
    }
}

/// Switch to per-key colors and upload the color table.
/// `colors` = (table index, color); unlisted keys stay dark.
pub fn set_key_colors(link: Link, brightness: u8, colors: &[(u8, Rgb)]) -> Vec<Packet> {
    let mode = per_key_block(brightness);
    let table = color_table(colors);
    match link {
        Link::Dongle => {
            let mut out = vec![Packet::dongle(dongle::frame(dongle::cmd::LIGHTING, 0x10, 0, &mode).unwrap(), 0)];
            // 21 slices of 28 bytes; the last one carries the remaining 16.
            for (i, chunk) in table.chunks(dongle::MAX_PAYLOAD).enumerate() {
                let len = chunk.len() as u8;
                out.push(Packet::dongle(dongle::frame(dongle::cmd::KEY_COLORS, len, i as u8, chunk).unwrap(), 2));
            }
            out
        }
        Link::Wired => {
            let mut out = wired_single_block(wired::cc::LIGHTING, &mode);
            let n = (COLOR_TABLE_LEN / wired::DATA_LEN) as u8;
            out.push(Packet::feature(wired::begin(), true));
            out.push(Packet::feature(wired::control(wired::cc::USER_LIGHT, n), true));
            for p in wired::data_packets(&table) {
                out.push(Packet::feature(p, false));
            }
            out.push(Packet::feature(wired::apply(), true));
            out.push(Packet::feature(wired::control(wired::cc::FINISH, 0), false));
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::Effect;

    #[test]
    fn dongle_effect_matches_original() {
        let l = Lighting { effect: Effect::Static, color: Rgb::new(0xFF, 0, 0), rainbow: false, brightness: 5, speed: 3, direction: 0 };
        let p = set_effect(Link::Dongle, &l);
        assert_eq!(p.len(), 1);
        let b = &p[0].bytes;
        assert_eq!(&b[..4], &[0x00, 0x05, 0x10, 0x00]);
        assert_eq!(&b[4..20], &[1, 0xFF, 0, 0, 0, 0, 0, 0, 0, 5, 3, 0, 0, 0, 0xAA, 0x55]);
        let sum = b[..32].iter().fold(0u8, |a, x| a.wrapping_add(*x));
        assert_eq!(b[32], sum);
    }

    #[test]
    fn wired_effect_sequence() {
        let p = set_effect(Link::Wired, &Lighting::default());
        let heads: Vec<(u8, u8)> = p.iter().map(|x| (x.bytes[1], x.bytes[2])).collect();
        assert_eq!(heads, vec![(0x04, 0x18), (0x04, 0x13), (11, 0xFF), (0x04, 0x02), (0x04, 0xF0)]);
        assert_eq!(p[1].bytes[9], 1);
        assert_eq!(p[2].bytes.len(), 65);
        assert_eq!(&p[2].bytes[15..17], &[0xAA, 0x55]);
        assert_eq!(p[2].kind, Kind::Feature { read_back: false });
        assert_eq!(p[4].kind, Kind::Feature { read_back: false });
    }

    #[test]
    fn dongle_key_colors() {
        let p = set_key_colors(Link::Dongle, 5, &[(1, Rgb::new(1, 2, 3))]);
        assert_eq!(p.len(), 22);
        assert_eq!(&p[0].bytes[4..5], &[0x80]);
        assert_eq!(&p[1].bytes[..4], &[0x00, 0x14, 0x1C, 0x00]);
        assert_eq!(&p[1].bytes[8..12], &[1, 1, 2, 3]);
        let last = &p[21].bytes;
        assert_eq!(&last[..4], &[0x00, 0x14, 0x10, 20]);
        assert_eq!(&last[4 + 14..4 + 16], &[0xAA, 0x55]);
    }

    #[test]
    fn wired_key_colors() {
        let p = set_key_colors(Link::Wired, 5, &[]);
        assert_eq!(p.len(), 5 + 2 + 9 + 2);
        assert_eq!((p[6].bytes[1], p[6].bytes[2], p[6].bytes[9]), (0x04, 0x23, 9));
    }
}
