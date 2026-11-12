//! Complete packet sequences for each operation, per connection type.
//!
//! The transport layer just executes a `Vec<Packet>`, and the CLI can print
//! the same list for `--dry-run`.

use crate::keymap::{self, KeyAction, Layer};
use crate::lighting::{color_table, effect_block, per_key_block, Lighting, Rgb, COLOR_TABLE_LEN};
use crate::settings::{self, Settings};
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

/// Upload the whole key map of one layer. Slots not listed keep their
/// factory function. (The original app also writes whole tables.)
pub fn set_key_map(link: Link, layer: Layer, entries: &[(u8, KeyAction)]) -> Vec<Packet> {
    let t = keymap::table(entries);
    match link {
        Link::Dongle => {
            let cmd = match layer {
                Layer::Base => dongle::cmd::KEYMAP_BASE,
                Layer::Fn => dongle::cmd::KEYMAP_FN,
            };
            t.chunks(dongle::MAX_PAYLOAD)
                .enumerate()
                .map(|(i, c)| Packet::dongle(dongle::frame(cmd, c.len() as u8, i as u8, c).unwrap(), 2))
                .collect()
        }
        Link::Wired => {
            let cc = match layer {
                Layer::Base => wired::cc::KEYMAP_BASE,
                Layer::Fn => wired::cc::KEYMAP_FN,
            };
            let mut out = vec![
                Packet::feature(wired::begin(), true),
                Packet::feature(wired::control(cc, (keymap::TABLE_LEN / wired::DATA_LEN) as u8), true),
            ];
            out.extend(wired::data_packets(&t).into_iter().map(|p| Packet::feature(p, false)));
            out.push(Packet::feature(wired::apply(), true));
            out.push(Packet::feature(wired::control(wired::cc::FINISH, 0), false));
            out
        }
    }
}

/// Return every key on both layers to its factory function.
pub fn reset_key_maps(link: Link) -> Vec<Packet> {
    match link {
        Link::Dongle => vec![Packet::dongle(dongle::frame(dongle::cmd::RESET_KEYS, 0, 0, &[]).unwrap(), 100)],
        Link::Wired => {
            let mut out = set_key_map(link, Layer::Base, &[]);
            out.extend(set_key_map(link, Layer::Fn, &[]));
            out
        }
    }
}

/// Write the system settings block.
pub fn set_settings(link: Link, s: &Settings) -> Vec<Packet> {
    match link {
        Link::Dongle => {
            vec![Packet::dongle(dongle::frame(dongle::cmd::SETTINGS, 0x10, 0, &settings::dongle_block(s)).unwrap(), 0)]
        }
        Link::Wired => {
            // Header `04 17 01 … 01`: data[2] = 1 as in the original app.
            let mut head = wired::control(wired::cc::SETTINGS, 1);
            head[3] = 0x01;
            vec![
                Packet::feature(wired::begin(), true),
                Packet::feature(head, true),
                Packet::feature(wired::data_packets(&settings::wired_block(s))[0], false),
                Packet::feature(wired::apply(), true),
            ]
        }
    }
}

/// Upload the macro storage buffer from `macros::storage`.
/// Sizes, trailer position and the odd last-packet LEN follow the
/// original app exactly.
pub fn set_macros(link: Link, storage: &[u8]) -> Vec<Packet> {
    let len = storage.len();
    match link {
        Link::Dongle => {
            let q = len / dongle::MAX_PAYLOAD;
            let n = q + 1 + usize::from(len % dongle::MAX_PAYLOAD != 0);
            let mut buf = storage.to_vec();
            buf.resize(n * dongle::MAX_PAYLOAD, 0);
            buf[n * dongle::MAX_PAYLOAD - 2..].copy_from_slice(&crate::BLOCK_TRAILER);
            buf.chunks(dongle::MAX_PAYLOAD)
                .enumerate()
                .map(|(i, c)| {
                    let l = if i == n - 1 { (len - q * dongle::MAX_PAYLOAD) as u8 } else { dongle::MAX_PAYLOAD as u8 };
                    Packet::dongle(dongle::frame(dongle::cmd::MACROS, l, i as u8, c).unwrap(), 2)
                })
                .collect()
        }
        Link::Wired => {
            let n = len / wired::DATA_LEN + if len % wired::DATA_LEN == 0 { 1 } else { 2 };
            let mut buf = storage.to_vec();
            buf.resize(n * wired::DATA_LEN, 0);
            buf[n * wired::DATA_LEN - 2..].copy_from_slice(&crate::BLOCK_TRAILER);
            let mut out = vec![
                Packet::feature(wired::control(wired::cc::BEGIN_MACRO, 0), true),
                Packet { pause_ms: 30, ..Packet::feature(wired::control(wired::cc::MACROS, n as u8), true) },
            ];
            let mut data: Vec<Packet> = wired::data_packets(&buf).into_iter().map(|p| Packet::feature(p, false)).collect();
            if let Some(last) = data.last_mut() {
                last.pause_ms = 30;
            }
            out.extend(data);
            out.push(Packet::feature(wired::apply(), true));
            out
        }
    }
}

/// Return the keyboard to its factory state: default lighting, every key
/// normal, no macros, default settings.
///
/// Receiver: command 0x0F followed by the default lighting, as the original
/// app does. USB cable: there is no single command, so every part is
/// rewritten with its default, again as the original app does.
pub fn factory_reset(link: Link) -> Vec<Packet> {
    let mut out = match link {
        Link::Dongle => vec![Packet::dongle(dongle::frame(dongle::cmd::FACTORY_RESET, 0, 0, &[]).unwrap(), 300)],
        Link::Wired => {
            let mut p = set_settings(link, &Settings::default());
            p.extend(reset_key_maps(link));
            p.extend(set_macros(link, &crate::macros::storage(&[]).unwrap()));
            p
        }
    };
    out.extend(set_effect(link, &Lighting::default()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lighting::Effect;

    #[test]
    fn macro_upload_sizes() {
        use crate::macros::{storage, Macro};
        let buf = storage(&[Some(Macro::parse("tap:A").unwrap())]).unwrap();
        assert_eq!(buf.len(), 0x1A8); // 424 bytes
        let w = set_macros(Link::Wired, &buf);
        // 424/64 = 6 rem 40 -> 8 packets
        assert_eq!(w[1].bytes[9], 8);
        assert_eq!(w.len(), 2 + 8 + 1);
        assert_eq!(&w[9].bytes[63..65], &[0xAA, 0x55]);
        assert_eq!(w[0].bytes[2], 0x19);
        let d = set_macros(Link::Dongle, &buf);
        // 424/28 = 15 rem 4 -> 17 packets, last LEN = 4
        assert_eq!(d.len(), 17);
        assert_eq!(&d[16].bytes[..4], &[0, 0x09, 4, 16]);
        assert_eq!(&d[15].bytes[..4], &[0, 0x09, 0x1C, 15]);
        assert_eq!(&d[16].bytes[30..32], &[0xAA, 0x55]);
    }

    #[test]
    fn dongle_key_map() {
        let p = set_key_map(Link::Dongle, Layer::Fn, &[(1, KeyAction::Disabled)]);
        assert_eq!(p.len(), 21);
        assert_eq!(&p[0].bytes[..12], &[0, 0x12, 0x1C, 0, 0, 0, 0, 0, 5, 3, 0, 0]);
        assert_eq!(&p[20].bytes[..4], &[0, 0x12, 0x10, 20]);
        assert_eq!(&p[20].bytes[18..20], &[0xAA, 0x55]);
    }

    #[test]
    fn wired_key_map() {
        let p = set_key_map(Link::Wired, Layer::Base, &[]);
        assert_eq!(p.len(), 2 + 9 + 2);
        assert_eq!((p[1].bytes[2], p[1].bytes[9]), (0x11, 9));
        assert_eq!(&p[10].bytes[63..65], &[0xAA, 0x55]);
        let r = reset_key_maps(Link::Wired);
        assert_eq!(r[14].bytes[2], 0x27);
        assert_eq!(&reset_key_maps(Link::Dongle)[0].bytes[..4], &[0, 0x1F, 0, 0]);
    }

    #[test]
    fn settings_plans() {
        let s = Settings::default();
        let d = set_settings(Link::Dongle, &s);
        assert_eq!(&d[0].bytes[..13], &[0, 0x07, 0x10, 0, 0, 1, 0, 0, 0, 0, 2, 0, 2]);
        let w = set_settings(Link::Wired, &s);
        assert_eq!(&w[1].bytes[..10], &[0, 0x04, 0x17, 0x01, 0, 0, 0, 0, 0, 1]);
        assert_eq!(&w[2].bytes[63..65], &[0xAA, 0x55]);
        assert_eq!(w.len(), 4);
    }

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
    fn factory_reset_plans() {
        let d = factory_reset(Link::Dongle);
        assert_eq!(d.len(), 2);
        assert_eq!(&d[0].bytes[..4], &[0, 0x0F, 0, 0]);
        assert_eq!(d[1].bytes[1], 0x05);
        let w = factory_reset(Link::Wired);
        let cmds: Vec<u8> = w.iter().filter(|p| p.bytes[1] == 0x04).map(|p| p.bytes[2]).collect();
        for c in [0x17, 0x11, 0x27, 0x15, 0x13] {
            assert!(cmds.contains(&c), "{c:02X} missing");
        }
    }

    #[test]
    fn wired_key_colors() {
        let p = set_key_colors(Link::Wired, 5, &[]);
        assert_eq!(p.len(), 5 + 2 + 9 + 2);
        assert_eq!((p[6].bytes[1], p[6].bytes[2], p[6].bytes[9]), (0x04, 0x23, 9));
    }
}
