use std::path::Path;

use sha2::{Digest, Sha256};

use crate::error::Td3Error;
use crate::library::model::{FileIndexEntry, FileIngestStatus, SnapshotSlot, Tag, TagKind};
use crate::library::store::LibraryStore;
use crate::pattern::Pattern;

pub(super) fn parse_by_format(
    fmt: &str,
    bytes: &[u8],
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<Pattern, Td3Error> {
    match fmt {
        "seq" => crate::formats::seq::import(bytes),
        "syx" => crate::formats::syx::import(bytes),
        "toml" => {
            let s = std::str::from_utf8(bytes)
                .map_err(|e| Td3Error::FormatError(format!("toml: {}", e)))?;
            crate::formats::toml_fmt::import(s)
        }
        "json" => {
            let s = std::str::from_utf8(bytes)
                .map_err(|e| Td3Error::FormatError(format!("json: {}", e)))?;
            crate::formats::json::import(s)
        }
        "steps" => {
            let s = std::str::from_utf8(bytes)
                .map_err(|e| Td3Error::FormatError(format!("steps: {}", e)))?;
            crate::formats::steps_txt::import(s)
        }
        "pat" => {
            let s = std::str::from_utf8(bytes)
                .map_err(|e| Td3Error::FormatError(format!("pat: {}", e)))?;
            crate::formats::pat::import(s)
        }
        "mid" => {
            // MIDI import may require a polyphony resolver; default to
            // lowest-pitch so ingest is deterministic and non-interactive.
            let mut resolver = crate::formats::mid_import::LowestPitchResolver;
            crate::formats::mid_import::import(bytes, midi_import_opts, &mut resolver)
        }
        other => Err(Td3Error::FormatError(format!(
            "ingest: no parser wired for format '{}'",
            other
        ))),
    }
}

/// Canonical hash of a Pattern - mirrors `library::duplicates::pattern_hash`
/// so duplicate detection across ingest + analyzer stays consistent.
pub(super) fn pattern_hash(pat: &Pattern) -> String {
    crate::library::duplicates::pattern_hash(pat)
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    let digest = h.finalize();
    let mut out = String::with_capacity(64);
    for b in digest.iter() {
        out.push_str(&format!("{:02x}", b));
    }
    out
}

pub(super) fn file_stem(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            path.file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("item")
                .to_string()
        })
}

pub(super) fn dashed_slot_key(group: u8, slot_addr: u8) -> String {
    let slot_num = slot_addr & 0x7;
    let side = slot_addr >> 3;
    format!(
        "G{}-P{}{}",
        group + 1,
        slot_num + 1,
        if side == 0 { 'A' } else { 'B' }
    )
}

pub(super) fn truncate_err(s: &str) -> String {
    if s.len() <= 200 {
        s.to_string()
    } else {
        let mut out = s[..200].to_string();
        out.push('…');
        out
    }
}

pub(crate) fn persist_snapshot_slot(
    store: &LibraryStore,
    entry: &mut FileIndexEntry,
    slot_row: SnapshotSlot,
) -> bool {
    let slot_key = slot_row.slot_key.clone();
    match store.upsert_snapshot_slot(slot_row) {
        Ok(_) => true,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("{}: slot: {}", slot_key, e)));
            false
        }
    }
}

/// Make sure an Auto-kind tag with this label exists. If the tag already
/// exists with a different kind, leave it alone - the user picked that kind.
pub(super) fn ensure_auto_tag(store: &LibraryStore, label: &str) -> Result<Tag, Td3Error> {
    store.ensure_tag_with_kind(label, TagKind::Auto)
}
