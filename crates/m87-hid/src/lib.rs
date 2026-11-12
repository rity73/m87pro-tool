//! HID discovery and transports for the M87 Pro keyboard.
//!
//! Talks to hid.dll / setupapi.dll directly (see `ffi`), so this crate has no
//! third-party dependencies.

mod device;
mod ffi;

use std::fmt;
use std::thread::sleep;
use std::time::Duration;

pub use device::{enumerate, HidDevice, HidInfo};
use m87_proto::plan::{Kind, Packet};
use m87_proto::{dongle, hex, wired, Link};

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    NotFound,
    OpenFailed(&'static str, std::io::Error),
    NoResponse(u8),
    KeyboardOffline,
    BadReply(String),
    Unsupported(&'static str),
}

impl fmt::Display for Error {
    // Each message says what happened and what to do about it.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "USB communication failed ({e}). Unplug the keyboard or receiver, plug it in again and retry"),
            Error::NotFound => write!(
                f,
                "no M87 Pro keyboard or 2.4 GHz receiver was found. Connect the USB cable or plug in the receiver. Bluetooth connections cannot be configured"
            ),
            Error::OpenFailed(what, e) => write!(
                f,
                "the {what} was found, but its settings port could not be opened ({e}). Close the original M87 Pro software if it is running, then retry"
            ),
            Error::NoResponse(cmd) => write!(
                f,
                "the keyboard did not answer (command 0x{cmd:02X}). Press a key to wake it, move it closer to the receiver, then retry"
            ),
            Error::KeyboardOffline => write!(
                f,
                "the receiver is plugged in, but the keyboard is off, asleep or out of range. Turn it on or press a key to wake it, then retry"
            ),
            Error::BadReply(r) => write!(f, "the keyboard sent an unexpected reply ({r}). Retry; if it keeps happening, reconnect the keyboard"),
            Error::Unsupported(link) => write!(f, "this is not available over the {link}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// What an HID interface of the keyboard is used for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// Configuration commands (lighting, keys, macros, settings).
    Config,
    /// Events from the keyboard to the PC.
    Events,
    /// Normal typing, mouse or media keys. Not used by this tool.
    Input,
}

impl Role {
    pub fn label(self) -> &'static str {
        match self {
            Role::Config => "settings",
            Role::Events => "events",
            Role::Input => "typing (not used)",
        }
    }
}

/// One HID interface (top-level collection) of the keyboard or receiver.
#[derive(Debug, Clone)]
pub struct Interface {
    pub link: Link,
    pub role: Role,
    pub info: HidInfo,
}

/// Same rules as the original app: a 65-byte feature report means the wired
/// settings port; 33-byte input and output reports mean the receiver's port.
fn role_of(link: Link, i: &HidInfo) -> Role {
    if i.usage_page == m87_proto::NOTIFY_USAGE_PAGE && i.usage == m87_proto::NOTIFY_USAGE {
        return Role::Events;
    }
    let config = match link {
        Link::Wired => i.feature_len as usize == wired::REPORT_LEN,
        Link::Dongle => i.output_len as usize == dongle::REPORT_LEN && i.input_len as usize == dongle::REPORT_LEN,
    };
    if config {
        Role::Config
    } else {
        Role::Input
    }
}

/// Every HID interface that belongs to the keyboard or its receiver.
pub fn list_interfaces() -> Vec<Interface> {
    let mut out: Vec<Interface> = enumerate(m87_proto::VENDOR_ID)
        .into_iter()
        .filter_map(|info| {
            let link = Link::from_product_id(info.product_id)?;
            Some(Interface { link, role: role_of(link, &info), info })
        })
        .collect();
    out.sort_by_key(|i| (i.link as u8, i.info.interface, i.info.usage_page, i.info.usage));
    out
}

/// Optional packet logger (for `--verbose`).
pub type Logger = Box<dyn Fn(&str)>;

/// An open settings channel to the keyboard.
pub struct Keyboard {
    link: Link,
    dev: HidDevice,
    log: Option<Logger>,
    /// Pause before each wired packet (the original app uses 20 ms).
    pub wired_delay: Duration,
}

impl Keyboard {
    /// Open the settings port. A USB cable is preferred over the receiver
    /// when both are connected.
    pub fn open() -> Result<Self> {
        let all = list_interfaces();
        if all.is_empty() {
            return Err(Error::NotFound);
        }
        for link in [Link::Wired, Link::Dongle] {
            let mut last_err = None;
            for c in all.iter().filter(|i| i.link == link && i.role == Role::Config) {
                match HidDevice::open(&c.info) {
                    Ok(dev) => {
                        return Ok(Keyboard { link, dev, log: None, wired_delay: Duration::from_millis(20) })
                    }
                    Err(e) => last_err = Some(e),
                }
            }
            if let Some(e) = last_err {
                return Err(Error::OpenFailed(link.label(), e));
            }
        }
        Err(Error::NotFound)
    }

    pub fn link(&self) -> Link {
        self.link
    }

    pub fn set_logger(&mut self, log: Logger) {
        self.log = Some(log);
    }

    fn trace(&self, dir: &str, bytes: &[u8]) {
        if let Some(log) = &self.log {
            log(&format!("{dir} {}", hex(bytes)));
        }
    }

    /// Drop input reports that arrived before our request.
    fn drain(&self) -> Result<()> {
        let mut buf = [0u8; dongle::REPORT_LEN];
        for _ in 0..16 {
            let n = self.dev.read_timeout(&mut buf, 0)?;
            if n == 0 {
                break;
            }
            self.trace("<< (old)", &buf[..n]);
        }
        Ok(())
    }

    /// Receiver only: send one 33-byte report and wait for its
    /// acknowledgement. Returns the reply without the report ID
    /// (CMD, LEN, IDX, data…).
    /// The 2.4 GHz link sometimes drops a packet or its reply, so a packet
    /// without a reply is sent again (up to 3 times in total). Every packet
    /// in this protocol is safe to repeat.
    pub fn dongle_transact(&self, report: &[u8; dongle::REPORT_LEN]) -> Result<Vec<u8>> {
        if self.link != Link::Dongle {
            return Err(Error::Unsupported(self.link.label()));
        }
        let mut last = Error::NoResponse(report[1]);
        for attempt in 0..3 {
            if attempt > 0 {
                self.trace("-- no reply, sending again", &[]);
                sleep(Duration::from_millis(20));
            }
            match self.dongle_once(report) {
                Err(e @ Error::NoResponse(_)) => last = e,
                other => return other,
            }
        }
        Err(last)
    }

    fn dongle_once(&self, report: &[u8; dongle::REPORT_LEN]) -> Result<Vec<u8>> {
        self.drain()?;
        self.trace(">>", report);
        if self.dev.write(report).is_err() {
            sleep(Duration::from_millis(10));
            self.dev.write(report)?;
        }
        sleep(Duration::from_millis(5));
        let mut buf = [0u8; dongle::REPORT_LEN];
        for _ in 0..20 {
            let n = self.dev.read_timeout(&mut buf, 10)?;
            if n > 1 {
                self.trace("<<", &buf[..n]);
                // buf[0] is the report ID (0).
                let reply = &buf[1..n];
                if dongle::is_ack(report, reply) {
                    return Ok(reply.to_vec());
                }
            }
            sleep(Duration::from_millis(10));
        }
        Err(Error::NoResponse(report[1]))
    }

    /// Receiver only: is the keyboard itself reachable?
    pub fn ping(&self) -> Result<()> {
        match self.dongle_transact(&dongle::ping()) {
            Err(Error::NoResponse(_)) => Err(Error::KeyboardOffline),
            other => other.map(|_| ()),
        }
    }

    /// Battery level in percent. Only available through the receiver.
    pub fn battery(&self) -> Result<u8> {
        let reply = match self.dongle_transact(&dongle::battery_query()) {
            Err(Error::NoResponse(_)) => return Err(Error::KeyboardOffline),
            other => other?,
        };
        dongle::parse_battery(&reply).ok_or_else(|| Error::BadReply(hex(&reply)))
    }

    /// Send a packet sequence built by `m87_proto::plan`. Stops at the first
    /// packet that fails. On the receiver, a missing reply to the first
    /// packet means the keyboard itself is unreachable.
    pub fn run(&self, packets: &[Packet]) -> Result<()> {
        for (i, p) in packets.iter().enumerate() {
            match p.kind {
                Kind::DongleAck => {
                    let report: &[u8; dongle::REPORT_LEN] = p
                        .bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::BadReply(format!("bad packet length {}", p.bytes.len())))?;
                    match self.dongle_transact(report) {
                        Err(Error::NoResponse(_)) if i == 0 => return Err(Error::KeyboardOffline),
                        other => other?,
                    };
                }
                Kind::Feature { read_back } => {
                    let report: &[u8; wired::REPORT_LEN] = p
                        .bytes
                        .as_slice()
                        .try_into()
                        .map_err(|_| Error::BadReply(format!("bad packet length {}", p.bytes.len())))?;
                    self.wired_send(report, read_back)?;
                }
            }
            if p.pause_ms > 0 {
                sleep(Duration::from_millis(p.pause_ms as u64));
            }
        }
        Ok(())
    }

    /// USB cable only: send one feature report, optionally followed by a
    /// feature read (the original app reads after control packets but does
    /// not check the content). Returns the read bytes when `read_back` is set.
    pub fn wired_send(&self, report: &[u8; wired::REPORT_LEN], read_back: bool) -> Result<Option<Vec<u8>>> {
        if self.link != Link::Wired {
            return Err(Error::Unsupported(self.link.label()));
        }
        sleep(self.wired_delay);
        self.trace(">>", report);
        self.dev.set_feature(report)?;
        if !read_back {
            return Ok(None);
        }
        sleep(self.wired_delay);
        let mut buf = [0u8; wired::REPORT_LEN];
        self.dev.get_feature(&mut buf)?;
        self.trace("<<", &buf);
        // Observed: the keyboard echoes `04 CC` and sets data[3] = 1 on success.
        if !wired::is_ack(report, &buf) {
            return Err(Error::BadReply(hex(&buf[..8])));
        }
        Ok(Some(buf.to_vec()))
    }
}
