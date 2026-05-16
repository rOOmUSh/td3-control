use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Snapshot pattern export
// ---------------------------------------------------------------------------
//
// Export N selected slots from a snapshot as individual pattern files in one
// or more formats. The backend creates a sub-folder named after the snapshot
// source (e.g. `idea_rbs_export`) inside the user-chosen target directory and
// writes each `{slot_key}.{ext}` into it (slot-key dashes stripped for clean
// filenames: G1-P1A -> G1P1A).
//
// `syx` and `sqs` are rejected - `syx` is transient scratch-slot data and
// `sqs` is a bank-level format with no single-pattern meaning. Supported:
// toml, json, steps_txt, pat, seq, mid, rbs.

#[derive(Deserialize)]
pub struct ExportSnapshotPatternsRequest {
    /// Slot keys in "G{g}-P{p}{side}" form, e.g. "G1-P1A". Must all exist
    /// and be non-empty in the snapshot.
    pub slot_keys: Vec<String>,
    /// One or more format ids. Each format produces one file per slot.
    pub formats: Vec<String>,
    /// Absolute path to the user-chosen destination folder. The backend
    /// creates a sub-folder inside it; the target must already exist.
    pub target_dir: String,
}

#[derive(Serialize)]
pub struct ExportSnapshotPatternsResponse {
    pub folder_path: String,
    pub file_count: u32,
    pub skipped: Vec<String>,
}

// ---------------------------------------------------------------------------
// Bank audition (play/stop a single LibraryItem on the device)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct PlayItemQuery {
    /// Legacy integer BPM. Kept for wire compatibility; if `centibpm` is
    /// supplied it wins.
    pub bpm: Option<u32>,
    /// Tempo in centi-BPM (BPM x 100).
    pub centibpm: Option<u32>,
}

impl PlayItemQuery {
    pub fn resolve_centibpm(&self) -> Option<u32> {
        self.centibpm
            .or_else(|| self.bpm.map(|b| b.saturating_mul(100)))
    }
}

#[derive(Serialize)]
pub struct PlayItemResponse {
    pub ok: bool,
    pub item_id: String,
    pub bpm: u32,
    pub centibpm: u32,
}

#[derive(Serialize)]
pub struct PlayingItemResponse {
    pub item_id: Option<String>,
}

// ---------------------------------------------------------------------------
