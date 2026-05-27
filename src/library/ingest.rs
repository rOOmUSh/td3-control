//! Ingest pipeline.
//!
//! Reads files from disk, delegates to the appropriate `formats::*` loader,
//! computes a content hash, and upserts a `LibraryItem` in the store.
//! `.sqs` full-bank files additionally create a `Snapshot` with 64 slot rows
//! and one LibraryItem per non-silent slot.
//!
//! The pipeline is intentionally boring:
//! - every file goes through `ingest_path`;
//! - every outcome produces a `FileIndexEntry` that is appended to the store;
//! - nothing panics on bad input - bad bytes become `FileIngestStatus::Failed`
//!   with a short error string.
//!
//! Callers manage `ImportBatch` creation + finalisation; this module only
//! operates on one path at a time so it stays testable without an HTTP layer.
#![allow(dead_code)]

use std::path::Path;

use crate::error::Td3Error;

use super::model::{FileIndexEntry, FileIngestStatus};
use super::scanner;
use super::store::{self, LibraryStore};

mod candidates;
mod derived_duplicates;
mod full_bank;
mod helpers;
mod import_order;
mod single_pattern;

#[allow(unused_imports)]
pub use candidates::is_candidate_filename;
pub use candidates::list_candidate_files;
pub use derived_duplicates::{native_truth_for_derived_path, record_derived_duplicate_path};
#[allow(unused_imports)]
pub(crate) use helpers::persist_snapshot_slot;
pub use import_order::sort_import_paths;

use full_bank::{process_rbs, process_sqs};
use helpers::truncate_err;
use single_pattern::process_single_pattern;

/// Hard cap on file size for the ingest pipeline. Pattern files are always
/// tiny (`.sqs` full-bank = ~8 KB); a 10 MB ceiling comfortably covers every
/// legitimate input while rejecting bomb/decompression-style abuse.
pub const MAX_INGEST_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// Outcome of processing a single path. Mirrors `FileIngestStatus` but carries
/// the resulting `FileIndexEntry` so the HTTP layer can reply with a per-path
/// breakdown for the UI.
#[derive(Debug, Clone)]
pub struct IngestOutcome {
    pub entry: FileIndexEntry,
}

/// Ingest a single path under the scope of `batch_id`. The entry is appended
/// to the store regardless of outcome so every scan leaves an audit trail.
///
/// Returns the written `FileIndexEntry`. Errors from the store itself (write
/// lock, disk) propagate as `Td3Error`; parse errors do NOT - they become
/// `FileIngestStatus::Failed` entries.
pub fn ingest_path(
    store: &LibraryStore,
    path: &Path,
    batch_id: &str,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<IngestOutcome, Td3Error> {
    let entry = classify_and_process(store, path, batch_id, midi_import_opts);
    store.append_file_index_entry(entry.clone())?;
    Ok(IngestOutcome { entry })
}

/// Re-run parsing for an existing `Failed` entry. Returns the updated entry.
/// If the entry's status is not `Failed` it is returned unchanged.
pub fn retry_failed(
    store: &LibraryStore,
    mut entry: FileIndexEntry,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> Result<FileIndexEntry, Td3Error> {
    if entry.status != FileIngestStatus::Failed {
        return Ok(entry);
    }
    let batch_id_copy = entry.batch_id.clone().unwrap_or_default();
    let path = std::path::PathBuf::from(&entry.path);
    let new_entry = classify_and_process(store, &path, &batch_id_copy, midi_import_opts);
    entry.status = new_entry.status;
    entry.error = new_entry.error;
    entry.format = new_entry.format;
    entry.size = new_entry.size;
    entry.duplicate_of = new_entry.duplicate_of;
    entry.item_id = new_entry.item_id;
    Ok(entry)
}

fn classify_and_process(
    store: &LibraryStore,
    path: &Path,
    batch_id: &str,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> FileIndexEntry {
    let path_str = path.to_string_lossy().to_string();
    let discovered_at = store::now_iso();

    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_lowercase();
    let (format_name, status) = scanner::classify(&filename);

    let mut entry = FileIndexEntry {
        path: path_str.clone(),
        size: 0,
        hash_sha256: None,
        discovered_at,
        format: format_name.clone(),
        status,
        error: None,
        batch_id: Some(batch_id.to_string()),
        duplicate_of: None,
        item_id: None,
    };

    if format_name.is_none() || status == FileIngestStatus::Unsupported {
        entry.status = FileIngestStatus::Unsupported;
        return entry;
    }

    let safe_path = match crate::path_safety::require_safe_user_path(path) {
        Ok(p) => p,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };
    let meta = match std::fs::metadata(&safe_path) {
        Ok(m) => m,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("metadata: {}", e)));
            return entry;
        }
    };
    if !meta.is_file() {
        entry.status = FileIngestStatus::Failed;
        entry.error = Some("not a regular file".to_string());
        return entry;
    }
    entry.size = meta.len();
    if entry.size > MAX_INGEST_FILE_SIZE {
        entry.status = FileIngestStatus::Failed;
        entry.error = Some(format!(
            "file too large: {} bytes exceeds {} byte cap",
            entry.size, MAX_INGEST_FILE_SIZE
        ));
        return entry;
    }

    let fmt = match format_name.as_deref() {
        Some(f) => f,
        None => {
            entry.status = FileIngestStatus::Unsupported;
            return entry;
        }
    };

    if fmt == "sqs" {
        return process_sqs(store, &safe_path, entry);
    }
    if fmt == "rbs" {
        return process_rbs(store, &safe_path, entry);
    }

    process_single_pattern(store, &safe_path, fmt, entry, midi_import_opts)
}
