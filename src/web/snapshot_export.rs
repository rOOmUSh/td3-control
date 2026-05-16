//! Multi-pattern export from a snapshot to a user-chosen folder.
//!
//! The handler in `bank_handlers::export_snapshot_patterns` uses this module
//! to keep the route layer thin and the business logic unit-testable.
//!
//! Contract:
//!   * Input: target directory (must exist), slot keys (subset of the
//!     snapshot's 64), format ids (subset of {toml, json, steps_txt, pat,
//!     seq, mid, rbs}).
//!   * Output: `{target_dir}/{folder_stem}_export/{slot}.{ext}` for every
//!     (non-empty slot) × (format) combination.
//!   * `slot` is the slot key with dashes removed: "G1-P1A" -> "G1P1A".
//!   * Empty slots are skipped and reported back to the caller - they are
//!     NOT an error, because the UI may have sent the full selection before
//!     verifying emptiness server-side.
//!
//! `syx` and `sqs` are forbidden: `syx` is scratch-slot-only transport data
//! and `sqs` is a bank-level format with no single-pattern meaning. Callers
//! must filter them out of the UI dropdown; we reject them here as a belt +
//! braces defense.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::Td3Error;
use crate::formats::{self, mid::MidiExportOptions, rbs};
use crate::pattern::{sysex_to_pattern, Pattern};

/// Pure helper: the list of format ids the UI (and this handler) accept for
/// per-slot export. Kept in one place so the backend test and the UI dropdown
/// can stay in lockstep.
pub const ALLOWED_FORMATS: &[&str] = &["toml", "json", "steps_txt", "pat", "seq", "mid", "rbs"];

#[derive(Debug)]
pub struct ExportRequest<'a> {
    pub target_dir: &'a Path,
    pub folder_stem: &'a str,
    pub slots: &'a [ExportSlot],
    pub formats: &'a [String],
    /// Env-resolved MIDI export options. The caller threads this in from
    /// `AppState::midi_export_options` so the env file drives every
    /// runtime export (instead of `MidiExportOptions::default()`'s
    /// hardcoded constants).
    pub midi_opts: &'a MidiExportOptions,
}

#[derive(Debug, Clone)]
pub struct ExportSlot {
    pub slot_key: String,
    /// 112-byte cached payload (body after the 3-byte device header), as
    /// returned by `LibraryStore::pattern_bytes_for`. When `None`, the slot
    /// is skipped and listed in `ExportResult::skipped`.
    pub payload: Option<Vec<u8>>,
}

#[derive(Debug)]
pub struct ExportResult {
    pub folder_path: PathBuf,
    pub file_count: u32,
    pub skipped: Vec<String>,
}

/// Run the export. Creates the output folder (idempotent), decodes every
/// non-empty slot once, and writes each (slot × format) combination.
pub fn run(req: &ExportRequest<'_>) -> Result<ExportResult, Td3Error> {
    validate_formats(req.formats)?;
    if req.slots.is_empty() {
        return Err(Td3Error::Other("no slots selected".into()));
    }
    let target_dir = crate::path_safety::require_safe_user_path(req.target_dir)?;
    if !target_dir.exists() {
        return Err(Td3Error::Other(format!(
            "target directory does not exist: {}",
            target_dir.display()
        )));
    }
    if !target_dir.is_dir() {
        return Err(Td3Error::Other(format!(
            "target path is not a directory: {}",
            target_dir.display()
        )));
    }

    let folder_name = format!("{}_export", sanitize_component(req.folder_stem));
    let out_dir = crate::path_safety::require_within_base(&target_dir, &folder_name)?;
    fs::create_dir_all(&out_dir)
        .map_err(|e| Td3Error::Other(format!("create {}: {}", out_dir.display(), e)))?;

    let midi_opts = req.midi_opts;
    let mut file_count: u32 = 0;
    let mut skipped: Vec<String> = Vec::new();

    for slot in req.slots {
        let Some(payload) = slot.payload.as_deref() else {
            skipped.push(slot.slot_key.clone());
            continue;
        };
        let pattern = decode_slot_payload(&slot.slot_key, payload)?;
        let slot_stem = slot_key_to_filename(&slot.slot_key);
        for fmt in req.formats {
            let (ext, bytes) = render_format(fmt, &pattern, &slot_stem, midi_opts)?;
            let file_name = format!("{}.{}", slot_stem, ext);
            let path = crate::path_safety::require_within_base(&out_dir, &file_name)?;
            fs::write(&path, &bytes)
                .map_err(|e| Td3Error::Other(format!("write {}: {}", path.display(), e)))?;
            file_count += 1;
        }
    }

    Ok(ExportResult {
        folder_path: out_dir,
        file_count,
        skipped,
    })
}

/// Reject formats outside the allowed set. `syx` and `sqs` are the common
/// attempted but forbidden inputs - call them out by name so the UI can show
/// a clear error if the belt + braces trips.
pub fn validate_formats(formats: &[String]) -> Result<(), Td3Error> {
    if formats.is_empty() {
        return Err(Td3Error::Other("no formats selected".into()));
    }
    for fmt in formats {
        let f = fmt.as_str();
        if f == "syx" {
            return Err(Td3Error::Other(
                "syx is transient scratch-slot data; not supported for snapshot export".into(),
            ));
        }
        if f == "sqs" {
            return Err(Td3Error::Other(
                "sqs is a bank-level format; use snapshot export for .sqs instead".into(),
            ));
        }
        if !ALLOWED_FORMATS.contains(&f) {
            return Err(Td3Error::Other(format!(
                "unsupported format '{}' (allowed: {})",
                f,
                ALLOWED_FORMATS.join(", ")
            )));
        }
    }
    Ok(())
}

/// "G1-P1A" -> "G1P1A". Keeps filenames short and filesystem-clean.
pub fn slot_key_to_filename(key: &str) -> String {
    key.replace('-', "")
}

/// Replace filesystem-unsafe chars and whitespace with `_`. Falls back to
/// `"snapshot"` if the stem is entirely made of junk.
pub fn sanitize_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_was_under = false;
    let bad = |c: char| matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|');
    for c in s.chars() {
        if bad(c) || c.is_whitespace() {
            if !prev_was_under {
                out.push('_');
                prev_was_under = true;
            }
        } else {
            out.push(c);
            prev_was_under = false;
        }
    }
    // Trim leading/trailing underscores so "idea.rbs" -> "idea.rbs" and
    // "  weird  name  " -> "weird_name".
    let trimmed = out.trim_matches('_').to_string();
    if trimmed.is_empty() {
        return "snapshot".to_string();
    }
    trimmed
}

/// Pull a validated `Pattern` from a cached 112-byte sidecar payload. The
/// marker bytes we synthesize here match the shape expected by `sysex_to_pattern`:
/// `[0x78, 0x00, 0x00]` + payload body.
fn decode_slot_payload(slot_key: &str, payload: &[u8]) -> Result<Pattern, Td3Error> {
    if payload.len() != 112 {
        return Err(Td3Error::Other(format!(
            "slot {} sidecar is {} bytes, expected 112",
            slot_key,
            payload.len()
        )));
    }
    let mut sysex = Vec::with_capacity(115);
    sysex.push(0x78);
    sysex.push(0x00);
    sysex.push(0x00);
    sysex.extend_from_slice(payload);
    sysex_to_pattern(&sysex)
        .map_err(|e| Td3Error::Other(format!("slot {} decode: {}", slot_key, e)))
}

fn render_format(
    fmt_id: &str,
    pattern: &Pattern,
    address: &str,
    midi_opts: &MidiExportOptions,
) -> Result<(&'static str, Vec<u8>), Td3Error> {
    match fmt_id {
        "toml" => Ok(("toml", formats::toml_fmt::export(pattern)?.into_bytes())),
        "json" => Ok(("json", formats::json::export(pattern)?.into_bytes())),
        "steps_txt" => Ok((
            "steps.txt",
            formats::steps_txt::export(pattern).into_bytes(),
        )),
        "pat" => Ok(("pat", formats::pat::export(pattern).into_bytes())),
        "seq" => Ok(("seq", formats::seq::export(pattern)?)),
        "mid" => Ok(("mid", formats::mid::export(pattern, address, midi_opts)?)),
        "rbs" => Ok(("rbs", rbs::export_single(clone_pattern(pattern)?)?)),
        other => Err(Td3Error::Other(format!("unsupported format '{}'", other))),
    }
}

/// `Pattern` is not `Clone`; rebuild via `Pattern::new` so we can hand an
/// owned value to `rbs::export_single` without loosening the domain type.
fn clone_pattern(p: &Pattern) -> Result<Pattern, Td3Error> {
    Pattern::new(p.triplet, p.active_steps, p.step)
}
