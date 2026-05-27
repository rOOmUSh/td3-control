use serde::{Deserialize, Serialize};

use super::WebPattern;

// ---------------------------------------------------------------------------
// Pattern import (file → WebPattern)
// ---------------------------------------------------------------------------

// Import accepts either a UTF-8 `content` string (toml / json / steps / pat)
// or a raw `bytes` array (seq / mid). JSON byte arrays keep the transport
// simple - no base64 dep and MIDI/SEQ files are small (≤ few KB). The
// handler picks the right field per format and rejects the request if the
// required one is missing.
#[derive(Deserialize)]
pub struct PatternImportRequest {
    #[serde(default)]
    pub content: Option<String>,
    #[serde(default)]
    pub bytes: Option<Vec<u8>>,
    pub format: String,
}

#[derive(Serialize, Deserialize)]
pub struct PatternImportResponse {
    pub pattern: WebPattern,
}

// ---------------------------------------------------------------------------
// Pattern parse-bank (sqs / rbs → snapshot-grid)
// ---------------------------------------------------------------------------
//
// The single-pattern page can import `.sqs` / `.rbs` files by first parsing
// the bank server-side and letting the user pick one slot from the full
// 64-slot grid. Every populated slot's WebPattern is returned inline so the
// browser can preview, audition, and commit without a second round-trip.
// Silent/empty slots carry `empty: true` and omit the pattern payload.

#[derive(Deserialize)]
pub struct PatternParseBankRequest {
    pub bytes: Vec<u8>,
    pub format: String,
}

#[derive(Serialize, Deserialize)]
pub struct PatternParseBankSlot {
    /// "G{g}-P{p}{A|B}" - matches the dashed slot_key used by the bank UI.
    pub slot_key: String,
    pub empty: bool,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<WebPattern>,
}

#[derive(Serialize, Deserialize)]
pub struct PatternParseBankResponse {
    pub slots: Vec<PatternParseBankSlot>,
}

// ---------------------------------------------------------------------------
// Pattern play-preview (sqs/rbs slot audition)
// ---------------------------------------------------------------------------
//
// Transient pattern audition for the import-bank picker. The WebPattern
// comes straight from `/pattern/parse-bank` - there is no library item, so
// the dedicated `/bank/items/:id/play` endpoint doesn't fit. The handler
// encodes → uploads to the scratch slot → starts the clock, exactly the
// same sequence as `bank_handlers::play_item`.

#[derive(Deserialize)]
pub struct PatternPlayPreviewRequest {
    pub pattern: WebPattern,
    /// Legacy integer BPM. Kept for wire compatibility; if `centibpm` is
    /// supplied it wins.
    #[serde(default)]
    pub bpm: Option<u32>,
    /// Tempo in centi-BPM (BPM x 100).
    #[serde(default)]
    pub centibpm: Option<u32>,
}

impl PatternPlayPreviewRequest {
    pub fn resolve_centibpm(&self) -> Option<u32> {
        self.centibpm
            .or_else(|| self.bpm.map(|b| b.saturating_mul(100)))
    }
}

#[derive(Serialize)]
pub struct PatternPlayPreviewResponse {
    pub ok: bool,
    pub bpm: u32,
    pub centibpm: u32,
}

// ---------------------------------------------------------------------------
// Pattern audition (host-sequenced, non-saving)
// ---------------------------------------------------------------------------
//
// Plays the supplied WebPattern by sending timed Note On/Off from the host,
// with no MIDI Start and no scratch-slot write - the device sequencer stays
// idle and device pattern memory is untouched. The opposite of
// PatternPlayPreviewRequest, which uploads to the scratch slot and starts the
// device clock.

#[derive(Deserialize)]
pub struct PatternAuditionRequest {
    pub pattern: WebPattern,
    /// Legacy integer BPM. Kept for wire compatibility; if `centibpm` is
    /// supplied it wins.
    #[serde(default)]
    pub bpm: Option<u32>,
    /// Tempo in centi-BPM (BPM x 100).
    #[serde(default)]
    pub centibpm: Option<u32>,
    /// Repeat the active-step cycle until stopped. Defaults to true.
    #[serde(default = "default_audition_looping")]
    pub looping: bool,
    #[serde(default)]
    pub target_epoch_micros: Option<u64>,
}

fn default_audition_looping() -> bool {
    true
}

impl PatternAuditionRequest {
    pub fn resolve_centibpm(&self) -> Option<u32> {
        self.centibpm
            .or_else(|| self.bpm.map(|b| b.saturating_mul(100)))
    }
}

#[derive(Serialize)]
pub struct PatternAuditionResponse {
    pub ok: bool,
    pub bpm: u32,
    pub centibpm: u32,
    pub looping: bool,
}

// ---------------------------------------------------------------------------
// Pattern export pool (batch → text formats)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExportPoolRequest {
    pub patterns: Vec<WebPattern>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ExportedFile {
    pub name: String,
    pub toml: String,
    pub json: String,
    pub steps: String,
}

#[derive(Serialize, Deserialize)]
pub struct ExportPoolResponse {
    pub files: Vec<ExportedFile>,
}

// ---------------------------------------------------------------------------
// Single-pattern export (main page IMPORT|EXPORT row)
// ---------------------------------------------------------------------------
//
// Mirror of pattern/import - the main page holds one pattern in state,
// POSTs { pattern, format } here, and the handler streams the exported
// file bytes back with a format-appropriate Content-Type. Filename is
// composed client-side from the current scratch coords.

#[derive(Deserialize)]
pub struct PatternExportRequest {
    pub pattern: WebPattern,
    pub format: String,
    #[serde(default)]
    pub patterns: Vec<WebPattern>,
    #[serde(default)]
    pub rbs_mode: Option<String>,
}

// ---------------------------------------------------------------------------
