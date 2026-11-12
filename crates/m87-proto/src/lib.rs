//! Packet encoding for the M87 Pro keyboard configuration protocol.
//!
//! This crate does no I/O. It only builds and checks byte buffers, so every
//! rule here can be unit-tested without a keyboard. See `docs/protocol.md`.

pub mod dongle;
pub mod keymap;
pub mod layout;
pub mod lighting;
pub mod plan;
pub mod settings;
pub mod wired;

/// SONiX (Microdia) vendor ID used by the keyboard and the dongle.
pub const VENDOR_ID: u16 = 0x0C45;
/// Product ID of the keyboard on a USB cable.
pub const PRODUCT_ID_WIRED: u16 = 0x8006;
/// Product ID of the 2.4 GHz USB receiver (dongle).
pub const PRODUCT_ID_DONGLE: u16 = 0xFEFE;

/// HID interface that carries configuration commands on a USB cable.
pub const INTERFACE_WIRED: i32 = 0;
/// HID interface that carries configuration commands through the dongle.
pub const INTERFACE_DONGLE: i32 = 3;

/// Usage page / usage of the interface where the keyboard sends events to the PC
/// (keyboard came online, went offline, host-side key actions).
pub const NOTIFY_USAGE_PAGE: u16 = 0xFFFF;
pub const NOTIFY_USAGE: u16 = 0x0001;

/// Trailer that ends every uploaded data block.
pub const BLOCK_TRAILER: [u8; 2] = [0xAA, 0x55];

/// How the keyboard is connected to this PC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Link {
    /// USB cable.
    Wired,
    /// 2.4 GHz wireless receiver.
    Dongle,
}

impl Link {
    pub fn from_product_id(pid: u16) -> Option<Self> {
        match pid {
            PRODUCT_ID_WIRED => Some(Link::Wired),
            PRODUCT_ID_DONGLE => Some(Link::Dongle),
            _ => None,
        }
    }

    /// Short label for display.
    pub fn label(self) -> &'static str {
        match self {
            Link::Wired => "USB cable",
            Link::Dongle => "2.4 GHz receiver",
        }
    }

    /// HID interface number that carries configuration commands.
    pub fn config_interface(self) -> i32 {
        match self {
            Link::Wired => INTERFACE_WIRED,
            Link::Dongle => INTERFACE_DONGLE,
        }
    }
}

/// Format bytes as space-separated hex, e.g. `00 02 00`.
pub fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 {
            s.push(' ');
        }
        s.push_str(&format!("{b:02X}"));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_from_pid() {
        assert_eq!(Link::from_product_id(0x8006), Some(Link::Wired));
        assert_eq!(Link::from_product_id(0xFEFE), Some(Link::Dongle));
        assert_eq!(Link::from_product_id(0x1234), None);
    }

    #[test]
    fn hex_format() {
        assert_eq!(hex(&[0x00, 0x0a, 0xff]), "00 0A FF");
        assert_eq!(hex(&[]), "");
    }
}
