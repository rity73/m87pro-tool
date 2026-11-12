//! One-line text field.

use ratatui::crossterm::event::KeyCode;

#[derive(Debug, Default, Clone)]
pub struct Input {
    chars: Vec<char>,
    /// Cursor position, in characters.
    pub cursor: usize,
}

impl Input {
    pub fn new(text: String) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let cursor = chars.len();
        Input { chars, cursor }
    }

    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn insert_str(&mut self, s: &str) {
        for c in s.chars().filter(|c| !c.is_control()) {
            self.chars.insert(self.cursor, c);
            self.cursor += 1;
        }
    }

    pub fn key(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char(c) => self.insert_str(&c.to_string()),
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.chars.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < self.chars.len() => {
                self.chars.remove(self.cursor);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.chars.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => self.cursor = self.chars.len(),
            _ => {}
        }
    }

    /// Visible slice for a field `width` columns wide and the cursor column in it.
    pub fn view(&self, width: usize) -> (String, usize) {
        let width = width.max(1);
        let start = self.cursor.saturating_sub(width - 1);
        let s: String = self.chars.iter().skip(start).take(width).collect();
        (s, self.cursor - start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing() {
        let mut i = Input::new("tap:A".into());
        i.key(KeyCode::Backspace);
        i.insert_str("B");
        assert_eq!(i.text(), "tap:B");
        i.key(KeyCode::Home);
        i.key(KeyCode::Delete);
        assert_eq!(i.text(), "ap:B");
        let (v, c) = Input::new("abcdefgh".into()).view(4);
        assert_eq!((v.as_str(), c), ("fgh", 3));
    }
}
