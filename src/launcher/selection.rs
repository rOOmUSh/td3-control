//! Group / Pattern / Side selection state for the launcher GUI.
//!
//! Mirrors the web UI's `G{1..4}-P{1..8}{A|B}` slot addressing. Holds raw
//! 1-indexed group/pattern values and a 0/1 side flag so the GUI can render
//! buttons without arithmetic in the view layer.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionState {
    pub group: u8,
    pub pattern: u8,
    pub side_b: bool,
}

impl SelectionState {
    #[allow(dead_code)]
    pub fn new(group: u8, pattern: u8, side_b: bool) -> Self {
        Self {
            group,
            pattern,
            side_b,
        }
    }

    pub fn label(&self) -> String {
        format!(
            "G{}-P{}{}",
            self.group,
            self.pattern,
            if self.side_b { 'B' } else { 'A' }
        )
    }

    pub fn from_label(s: &str) -> Option<Self> {
        let cleaned: String = s
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '-')
            .collect();
        let bytes = cleaned.as_bytes();
        if bytes.len() < 5 {
            return None;
        }
        if !bytes[0].eq_ignore_ascii_case(&b'G') || !bytes[2].eq_ignore_ascii_case(&b'P') {
            return None;
        }
        let group = (bytes[1] as char).to_digit(10)? as u8;
        let pattern = (bytes[3] as char).to_digit(10)? as u8;
        let side = bytes[4].to_ascii_uppercase();
        let side_b = match side {
            b'A' => false,
            b'B' => true,
            _ => return None,
        };
        if !(1..=4).contains(&group) || !(1..=8).contains(&pattern) {
            return None;
        }
        Some(Self {
            group,
            pattern,
            side_b,
        })
    }
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            group: 1,
            pattern: 1,
            side_b: false,
        }
    }
}
