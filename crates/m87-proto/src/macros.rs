//! Hardware macros stored in the keyboard (`docs/protocol.md` section 2.5).
//!
//! Storage buffer:
//! ```text
//! 0x000  100 × [off_lo, off_hi, 0, 0]   offset of each macro, FF FF FF FF if empty
//! 0x190  macro 1: [cnt_lo, cnt_hi, 0,0,0,0,0,0] then cnt × 4-byte entries
//!        ...
//! ```
//! Each step is followed by a delay entry, and `cnt` counts both.

use crate::layout;

/// Number of macro slots.
pub const SLOTS: usize = 100;
/// Largest storage buffer the original app builds.
pub const BUFFER_LEN: usize = 0xE00;
/// Shortest delay the keyboard is given after a step.
pub const MIN_DELAY_MS: u16 = 10;
/// Delay used when the text does not give one.
pub const DEFAULT_DELAY_MS: u16 = 20;

/// Mouse buttons usable in macros.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Right,
    Middle,
}

impl Button {
    fn bit(self) -> u8 {
        match self {
            Button::Left => 0x01,
            Button::Right => 0x02,
            Button::Middle => 0x04,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Button::Left => "left",
            Button::Right => "right",
            Button::Middle => "middle",
        }
    }
    fn parse(s: &str) -> Option<Button> {
        [Button::Left, Button::Right, Button::Middle].into_iter().find(|b| b.name().eq_ignore_ascii_case(s))
    }
}

/// One step of a macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    KeyDown(u8),
    KeyUp(u8),
    MouseDown(Button),
    MouseUp(Button),
}

/// A step and the pause after it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    pub action: Action,
    pub delay_ms: u16,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Macro {
    pub steps: Vec<Step>,
}

fn key_usage(name: &str) -> Result<u8, String> {
    let k = layout::find(name).ok_or_else(|| format!("unknown key \"{name}\". Run `m87 keys` to see key names"))?;
    if k.usage == 0xAF {
        return Err("the Fn key cannot be used in a macro".into());
    }
    Ok(k.usage)
}

fn key_name(usage: u8) -> String {
    layout::KEYS.iter().find(|k| k.usage == usage).map(|k| k.id.to_string()).unwrap_or(format!("0x{usage:02X}"))
}

impl Macro {
    /// Parse the text form, a comma-separated list of steps:
    ///
    /// * `tap:A` — press and release A
    /// * `down:LShift`, `up:LShift` — hold and release a key
    /// * `click:left`, `mouse-down:right`, `mouse-up:right`
    /// * `wait:200` — extra pause in milliseconds before the next step
    ///
    /// Every step is followed by a 20 ms pause unless `wait` changes it.
    pub fn parse(text: &str) -> Result<Macro, String> {
        let mut steps: Vec<Step> = Vec::new();
        let push = |steps: &mut Vec<Step>, action| steps.push(Step { action, delay_ms: DEFAULT_DELAY_MS });
        for raw in text.split(',') {
            let tok = raw.trim();
            if tok.is_empty() {
                continue;
            }
            let (kind, arg) = tok.split_once(':').ok_or_else(|| format!("\"{tok}\" must look like KIND:VALUE, for example tap:A"))?;
            let arg = arg.trim();
            match kind.trim().to_ascii_lowercase().as_str() {
                "tap" => {
                    let u = key_usage(arg)?;
                    push(&mut steps, Action::KeyDown(u));
                    push(&mut steps, Action::KeyUp(u));
                }
                "down" => push(&mut steps, Action::KeyDown(key_usage(arg)?)),
                "up" => push(&mut steps, Action::KeyUp(key_usage(arg)?)),
                "click" | "mouse-down" | "mouse-up" => {
                    let b = Button::parse(arg).ok_or_else(|| format!("\"{arg}\" is not a mouse button; use left, right or middle"))?;
                    match kind {
                        "click" => {
                            push(&mut steps, Action::MouseDown(b));
                            push(&mut steps, Action::MouseUp(b));
                        }
                        "mouse-down" => push(&mut steps, Action::MouseDown(b)),
                        _ => push(&mut steps, Action::MouseUp(b)),
                    }
                }
                "wait" => {
                    let ms: u16 = arg.parse().map_err(|_| format!("\"{arg}\" is not a number of milliseconds (up to 65535)"))?;
                    let last = steps.last_mut().ok_or("a macro cannot start with wait")?;
                    last.delay_ms = last.delay_ms.saturating_add(ms);
                }
                other => {
                    return Err(format!("\"{other}\" is not a step. Use tap, down, up, click, mouse-down, mouse-up or wait"))
                }
            }
        }
        if steps.is_empty() {
            return Err("the macro has no steps".into());
        }
        Ok(Macro { steps })
    }

    /// Text form; `parse(to_text())` gives the same macro.
    pub fn to_text(&self) -> String {
        let mut out: Vec<String> = Vec::new();
        let mut i = 0;
        while i < self.steps.len() {
            let s = self.steps[i];
            let next = self.steps.get(i + 1).copied();
            let (text, delay, used) = match (s.action, next) {
                (Action::KeyDown(a), Some(Step { action: Action::KeyUp(b), delay_ms }))
                    if a == b && s.delay_ms == DEFAULT_DELAY_MS =>
                {
                    (format!("tap:{}", key_name(a)), delay_ms, 2)
                }
                (Action::MouseDown(a), Some(Step { action: Action::MouseUp(b), delay_ms }))
                    if a == b && s.delay_ms == DEFAULT_DELAY_MS =>
                {
                    (format!("click:{}", a.name()), delay_ms, 2)
                }
                (Action::KeyDown(a), _) => (format!("down:{}", key_name(a)), s.delay_ms, 1),
                (Action::KeyUp(a), _) => (format!("up:{}", key_name(a)), s.delay_ms, 1),
                (Action::MouseDown(a), _) => (format!("mouse-down:{}", a.name()), s.delay_ms, 1),
                (Action::MouseUp(a), _) => (format!("mouse-up:{}", a.name()), s.delay_ms, 1),
            };
            out.push(text);
            if delay > DEFAULT_DELAY_MS {
                out.push(format!("wait:{}", delay - DEFAULT_DELAY_MS));
            }
            i += used;
        }
        out.join(", ")
    }

    /// 4-byte entries: each step, then its delay.
    fn entries(&self) -> Vec<[u8; 4]> {
        let mut out = Vec::with_capacity(self.steps.len() * 2);
        for s in &self.steps {
            out.push(match s.action {
                Action::KeyDown(u) => [0, 0, u, 0xB0],
                Action::KeyUp(u) => [0, 0, u, 0x30],
                Action::MouseDown(b) => [0, 0, b.bit(), 0x90],
                Action::MouseUp(b) => [0, 0, b.bit(), 0x10],
            });
            let d = s.delay_ms.max(MIN_DELAY_MS);
            out.push([d as u8, (d >> 8) as u8, 0, 0x50]);
        }
        out
    }
}

/// Build the storage buffer (without padding or trailer). `macros[i]` goes
/// to slot `i`; `None` marks an empty slot. Returns an error when the
/// macros do not fit.
pub fn storage(macros: &[Option<Macro>]) -> Result<Vec<u8>, String> {
    if macros.len() > SLOTS {
        return Err(format!("at most {SLOTS} macros can be stored"));
    }
    let mut buf = vec![0u8; SLOTS * 4];
    for (i, m) in macros.iter().enumerate() {
        let entries = m.as_ref().map(|m| m.entries()).unwrap_or_default();
        if entries.is_empty() {
            buf[i * 4..i * 4 + 4].copy_from_slice(&[0xFF; 4]);
            continue;
        }
        let off = buf.len();
        buf[i * 4] = off as u8;
        buf[i * 4 + 1] = (off >> 8) as u8;
        let cnt = entries.len();
        buf.extend_from_slice(&[cnt as u8, (cnt >> 8) as u8, 0, 0, 0, 0, 0, 0]);
        for e in entries {
            buf.extend_from_slice(&e);
        }
    }
    // Leave room for the padding packet and its trailer.
    if buf.len() + 64 > BUFFER_LEN {
        return Err(format!(
            "the macros need {} bytes but the keyboard has room for about {}. Remove steps or macros",
            buf.len(),
            BUFFER_LEN - 64
        ));
    }
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_text() {
        let m = Macro::parse("tap:H, tap:I, wait:200, down:LShift, tap:A, up:LShift, click:left").unwrap();
        assert_eq!(m.steps.len(), 10);
        assert_eq!(m.steps[3].delay_ms, 220);
        assert_eq!(m.to_text(), "tap:H, tap:I, wait:200, down:LShift, tap:A, up:LShift, click:left");
        assert_eq!(Macro::parse(&m.to_text()).unwrap(), m);
    }

    #[test]
    fn parse_errors() {
        assert!(Macro::parse("").unwrap_err().contains("no steps"));
        assert!(Macro::parse("wait:10").unwrap_err().contains("cannot start"));
        assert!(Macro::parse("tap:Fn").unwrap_err().contains("Fn"));
        assert!(Macro::parse("press:A").unwrap_err().contains("not a step"));
        assert!(Macro::parse("A").unwrap_err().contains("KIND:VALUE"));
        assert!(Macro::parse("click:side").unwrap_err().contains("mouse button"));
    }

    #[test]
    fn storage_layout() {
        let m = Macro::parse("tap:A").unwrap();
        let buf = storage(&[Some(m), None]).unwrap();
        assert_eq!(&buf[0..4], &[0x90, 0x01, 0, 0]);
        assert_eq!(&buf[4..8], &[0xFF; 4]);
        assert_eq!(&buf[8..12], &[0, 0, 0, 0]);
        assert_eq!(&buf[0x190..0x198], &[4, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&buf[0x198..0x1A8], &[0, 0, 0x04, 0xB0, 20, 0, 0, 0x50, 0, 0, 0x04, 0x30, 20, 0, 0, 0x50]);
        assert_eq!(buf.len(), 0x1A8);
    }

    #[test]
    fn storage_too_big() {
        let long = vec!["tap:A"; 400].join(",");
        let m = Macro::parse(&long).unwrap();
        assert!(storage(&[Some(m)]).unwrap_err().contains("room"));
    }
}
