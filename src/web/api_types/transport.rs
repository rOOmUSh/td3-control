use serde::{Deserialize, Serialize};

use crate::formats;
use crate::step;

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BpmRequest {
    /// Legacy integer BPM. Kept for wire compatibility with older clients
    /// that don't know about centi-BPM. When both fields are supplied
    /// `centibpm` wins.
    #[serde(default)]
    pub bpm: Option<u32>,
    /// Tempo in centi-BPM (BPM x 100). Preferred field; supplies the
    /// fractional precision required for the .00 BPM toggle.
    #[serde(default)]
    pub centibpm: Option<u32>,
    #[serde(default)]
    pub target_epoch_micros: Option<u64>,
}

impl BpmRequest {
    /// Resolve the request to a single centi-BPM value, preferring the
    /// explicit `centibpm` field. Returns `None` if neither field was
    /// supplied so callers can reject the request with a precise error.
    pub fn resolve_centibpm(&self) -> Option<u32> {
        self.centibpm
            .or_else(|| self.bpm.map(|b| b.saturating_mul(100)))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportResponse {
    pub ok: bool,
    pub started_at_epoch_ms: u64,
    pub transport_id: u64,
    pub ppqn: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportWrapPulseRequest {
    pub transport_id: u64,
    pub anchor_epoch_ms: u64,
    pub wrap_index: u64,
    pub active_steps: u8,
    pub triplet: bool,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransportWrapPulseResponse {
    pub ok: bool,
    pub transport_id: u64,
    pub wrap_index: u64,
    pub wrap_epoch_ms: u64,
    pub server_epoch_ms: u64,
    pub ppqn: u32,
}

// ---------------------------------------------------------------------------
// Note preview
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct NotePreviewRequest {
    pub note: String,
    pub transpose: String,
    pub accent: bool,
}

impl NotePreviewRequest {
    pub fn midi_note(&self) -> Result<u8, String> {
        let note_index = formats::NOTE_NAMES
            .iter()
            .position(|name| name.eq_ignore_ascii_case(self.note.trim()))
            .map(|idx| idx as u8)
            .ok_or_else(|| format!("unknown note '{}'", self.note))?;

        let transpose = step::Transpose::from_contract(&self.transpose)
            .map_err(|_| format!("unknown transpose '{}'", self.transpose))?;

        // Mirror midi_note_number(): 12 + note + (transpose * 12) + octave_offset(12)
        Ok((24u16 + note_index as u16 + transpose.pitch_base_offset() as u16).min(127) as u8)
    }
}

#[derive(Serialize, Deserialize)]
pub struct NotePreviewResponse {
    pub ok: bool,
}
