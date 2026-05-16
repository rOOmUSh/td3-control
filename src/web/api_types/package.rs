use serde::{Deserialize, Serialize};

use super::WebPattern;

// Progression package export (Supporting Bassline)
// ---------------------------------------------------------------------------
//
// The browser posts the full
// in-memory package payload (acid + basslines + metadata + format selection)
// and the backend assembles/writes the ZIP. It is explicitly forbidden to
// re-read IndexedDB from the server side.

#[derive(Deserialize, Default)]
#[serde(default)]
pub struct CombinedFormats {
    pub rbs: bool,
    pub sqs: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPackageRequest {
    pub package_id: String,
    pub formats: Vec<String>,
    #[serde(default)]
    pub combined_formats: CombinedFormats,
    pub scale_name: String,
    /// Exactly 4 acid patterns (P1..P4) matching the in-memory progression.
    pub acid_patterns: Vec<WebPattern>,
    /// Exactly 4 supporting basslines (P1_BASSLINE..P4_BASSLINE). Each entry
    /// is the *active* archetype for that position - used for per-pattern
    /// exports and as the combined-export fallback when `basslines_full` is
    /// absent.
    pub basslines: Vec<WebPattern>,
    /// Optional full archetype matrix: exactly 20 basslines when present
    /// (5 archetypes × 4 positions, position-major × archetype-minor order:
    /// [P1.pedal, P1.rootPulse, P1.offbeat, P1.shadow, P1.arpeggio,
    ///  P2.pedal, …, P4.arpeggio]). When supplied, combined SQS/RBS exports
    /// place all 20 at B-side / Device 2 slots (G1P1..G3P4); when absent,
    /// combined exports fall back to the 4-slot `basslines` layout.
    #[serde(default)]
    pub basslines_full: Option<Vec<WebPattern>>,
    /// Optional override for the server's working folder. When absent the
    /// backend writes next to its current working directory.
    #[serde(default)]
    pub working_dir: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportPackageResponse {
    pub ok: bool,
    pub package_id: String,
    pub zip_name: String,
    pub saved_path: String,
    pub created_at: String,
    pub file_count: u32,
}
