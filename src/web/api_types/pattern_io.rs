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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub centibpm: Option<u32>,
    /// StepDSL v1.1 lanes and page state; absent for every other format
    /// and for a v1 document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps_meta: Option<super::WebStepsMeta>,
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
    /// Step-relative host-audition note gate. Higher values hold ordinary
    /// notes longer. Omitted values retain the legacy half-step gate.
    /// `noteDecayPercent` and `note_decay_percent` are the names this
    /// field carried before the control was renamed from DECAY to GATE.
    /// A caller written against either still works.
    #[serde(
        default = "default_gate_percent",
        alias = "gatePercent",
        alias = "noteDecayPercent",
        alias = "note_decay_percent"
    )]
    pub gate_percent: u32,
    /// Repeat the active-step cycle until stopped. Defaults to true.
    #[serde(default = "default_audition_looping")]
    pub looping: bool,
    #[serde(default, alias = "targetEpochMicros")]
    pub target_epoch_micros: Option<u64>,
    /// Active host-audition schedule generation expected by an update.
    /// Omitted values retain compatibility with callers that do not yet
    /// guard against a rollover race.
    #[serde(default, alias = "expectedScheduleGeneration")]
    pub expected_schedule_generation: Option<u64>,
    /// Optional NO-LIVE triplet morph amount, an integer 0 through 100.
    /// Omitted means legacy audition behavior, including the native
    /// `pattern.triplet` path. Explicit 0 selects the custom straight
    /// endpoint and is accepted only for an eligible straight
    /// source. Amounts above 0 require `activeSteps` of 4, 8, 12, or 16 and
    /// `triplet == false`.
    #[serde(default, alias = "tripletMorphPercent")]
    pub triplet_morph_percent: Option<u32>,
    /// MIDI channel, 1 through 16, to address the device on. Omitted
    /// means the channel resolved from `MIDI_DEVICE_CHANNEL`, so a caller
    /// written before the transport bar gained the control is unaffected.
    #[serde(default, alias = "midiChannel")]
    pub midi_channel: Option<u32>,
    /// Per-step ordinary-note gate, exactly 16 values of 1 through 100.
    /// Present means each note group uses the gate at its first step
    /// instead of `gate_percent`. Rejected together with a triplet morph
    /// amount.
    #[serde(default, alias = "stepGates")]
    pub step_gates: Option<Vec<u32>>,
    /// Per-step Filter Cutoff, exactly 16 values of 0 through 127.
    /// Present means a Control Change 74 is sent at the start of every
    /// active step. Rejected together with a triplet morph amount.
    #[serde(default, alias = "stepCutoffs")]
    pub step_cutoffs: Option<Vec<u32>>,
}

fn default_audition_looping() -> bool {
    true
}

fn default_gate_percent() -> u32 {
    50
}

impl PatternAuditionRequest {
    pub fn resolve_centibpm(&self) -> Option<u32> {
        self.centibpm
            .or_else(|| self.bpm.map(|b| b.saturating_mul(100)))
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatternAuditionResponse {
    pub ok: bool,
    pub bpm: u32,
    pub centibpm: u32,
    pub looping: bool,
    /// Active host-audition schedule generation. Starts at zero and advances
    /// when a queued next-cycle schedule is installed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_generation: Option<u64>,
    /// Wall-clock time when an update was installed by the audition thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_at_epoch_micros: Option<u64>,
    /// Wall-clock epoch of the active cycle containing the applied update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_epoch_micros: Option<u64>,
    /// Duration of one active-step cycle after the update was applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle_period_micros: Option<u64>,
    /// Cycle phase at `effective_at_epoch_micros` after tempo scaling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase_micros: Option<u64>,
    /// Morph diagnostics, present only when the request carried
    /// `tripletMorphPercent`. Flattened so the wire fields stay flat
    /// camelCase keys.
    #[serde(flatten)]
    pub triplet_morph: Option<super::TripletMorphDiagnostics>,
}

// ---------------------------------------------------------------------------
// Pattern export pool (batch → text formats)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ExportPoolRequest {
    pub patterns: Vec<WebPattern>,
    #[serde(default)]
    pub centibpm: Option<u32>,
    /// One entry per pattern, in order; a missing or shorter list means
    /// defaults for the patterns it does not cover.
    #[serde(default, alias = "stepsMeta")]
    pub steps_meta: Vec<super::WebStepsMeta>,
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
    pub centibpm: Option<u32>,
    #[serde(default)]
    pub patterns: Vec<WebPattern>,
    #[serde(default)]
    pub rbs_mode: Option<String>,
    /// StepDSL v1.1 lanes and page state for `steps_txt`; ignored by
    /// every other format. Absent means defaults.
    #[serde(default, alias = "stepsMeta")]
    pub steps_meta: Option<super::WebStepsMeta>,
}

// ---------------------------------------------------------------------------
