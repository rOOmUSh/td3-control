//! StepDSL (`.steps.txt`) codec.
//!
//! Two document tags are read: `td3-stepdsl-v1` (pattern rows, optional
//! `bpm`) and `td3-stepdsl-v1.1`, which adds per-step `CO` (Filter Cutoff,
//! 0-127) and `GT` (gate percent, 1-100) row fields plus the header keys
//! `pattern_co_lane`, `pattern_gt_lane`, `triplet_morph`,
//! `triplet_morph_percentage`, and `live_update`. Every export writes v1.1.
//! A v1 document parses as before with an empty [`StepsTxtMeta`].

mod bpm;
mod header;
mod lanes;
mod parse;
mod render;
mod row;

use crate::error::Td3Error;
use crate::pattern::Pattern;
use crate::step;

pub use bpm::centibpm_from_integer_bpm;

/// Tag of documents written before per-step lanes existed.
pub const STEPDSL_TAG_V1: &str = "td3-stepdsl-v1";
/// Tag written by every export and accepted alongside v1.
pub const STEPDSL_TAG_V1_1: &str = "td3-stepdsl-v1.1";

/// Default Filter Cutoff written when no lane data is supplied.
pub const DEFAULT_CUTOFF: u8 = 64;
/// Default gate percent written when no lane data is supplied.
pub const DEFAULT_GATE: u8 = 50;

/// Metadata read from a document beyond the pattern itself. Every field
/// is absent for a v1 document or when the v1.1 field was missing or
/// unusable (see the import rules in `lanes.rs`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StepsTxtMeta {
    pub centibpm: Option<u32>,
    /// Per-step Filter Cutoff, present only when every active row carried
    /// a numeric `CO`.
    pub cutoff: Option<[u8; step::Step::COUNT]>,
    /// Per-step gate percent, present only when every active row carried
    /// a numeric `GT`.
    pub gate: Option<[u8; step::Step::COUNT]>,
    /// `pattern_co_lane`, or the all-rows-equal heuristic when the key is
    /// absent and the lane is present.
    pub cutoff_lane_on: Option<bool>,
    pub gate_lane_on: Option<bool>,
    /// `Some(percent)` when `triplet_morph=on` with a usable percentage.
    pub triplet_morph_percent: Option<u8>,
    pub live_update: Option<bool>,
}

/// A parsed document: the pattern, its optional tempo, and the metadata.
#[derive(Debug)]
pub struct StepsTxtDocument {
    pub pattern: Pattern,
    pub centibpm: Option<u32>,
    pub meta: StepsTxtMeta,
}

/// Everything an export writes beyond the pattern rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepsTxtExportMeta {
    pub cutoff: [u8; step::Step::COUNT],
    pub gate: [u8; step::Step::COUNT],
    pub cutoff_lane_on: bool,
    pub gate_lane_on: bool,
    /// `Some(percent)` writes `triplet_morph=on`; `None` writes `off` and `0`.
    pub triplet_morph_percent: Option<u8>,
    pub live_update: bool,
}

impl Default for StepsTxtExportMeta {
    fn default() -> Self {
        Self {
            cutoff: [DEFAULT_CUTOFF; step::Step::COUNT],
            gate: [DEFAULT_GATE; step::Step::COUNT],
            cutoff_lane_on: false,
            gate_lane_on: false,
            triplet_morph_percent: None,
            live_update: false,
        }
    }
}

/// Export without tempo metadata. Kept for callers that never had a BPM.
#[allow(dead_code)] // Public compatibility helper; production saves include BPM.
pub fn export(pattern: &Pattern) -> String {
    render::render(pattern, None, &StepsTxtExportMeta::default())
}

/// Export with canonical StepDSL BPM metadata and default lanes.
pub fn export_with_bpm(pattern: &Pattern, centibpm: u32) -> Result<String, Td3Error> {
    export_with_meta(pattern, centibpm, &StepsTxtExportMeta::default())
}

/// Export using an integer BPM supplied by CLI and backend configuration.
pub fn export_with_integer_bpm(pattern: &Pattern, bpm: u32) -> Result<String, Td3Error> {
    let centibpm = centibpm_from_integer_bpm(bpm)?;
    export_with_bpm(pattern, centibpm)
}

/// Export with tempo and explicit lane, morph, and LIVE metadata.
pub fn export_with_meta(
    pattern: &Pattern,
    centibpm: u32,
    meta: &StepsTxtExportMeta,
) -> Result<String, Td3Error> {
    pattern.validate()?;
    let bpm = bpm::format_bpm_centibpm(centibpm)?;
    Ok(render::render(pattern, Some(&bpm), meta))
}

/// Import the pattern only.
pub fn import(data: &str) -> Result<Pattern, Td3Error> {
    Ok(import_document(data)?.pattern)
}

/// Import a document with its tempo and metadata.
pub fn import_document(data: &str) -> Result<StepsTxtDocument, Td3Error> {
    parse::import_document(data)
}
