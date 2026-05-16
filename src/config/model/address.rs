/// Pattern slot address (0-indexed patgroup, slot, side).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatternAddress {
    pub patgroup: u8,
    pub slot: u8,
    pub side: u8,
}

impl PatternAddress {
    /// Human-readable label, e.g. "G1-P1A".
    pub fn label(&self) -> String {
        let side_letter = if self.side == 0 { 'A' } else { 'B' };
        format!("G{}-P{}{}", self.patgroup + 1, self.slot + 1, side_letter)
    }
}

pub type ScratchPattern = PatternAddress;
