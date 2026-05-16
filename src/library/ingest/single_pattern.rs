use std::path::Path;

use crate::library::ids;
use crate::library::model::{
    AnalysisStatus, DuplicateStatus, FileIndexEntry, FileIngestStatus, LibraryItem, SourceKind,
};
use crate::library::store::{self, LibraryStore};

use super::helpers::{
    ensure_auto_tag, file_stem, parse_by_format, pattern_hash, sha256_hex, truncate_err,
};

pub(super) fn process_single_pattern(
    store: &LibraryStore,
    path: &Path,
    fmt: &str,
    mut entry: FileIndexEntry,
    midi_import_opts: &crate::formats::mid_import::MidiImportOptions,
) -> FileIndexEntry {
    let path = match crate::path_safety::require_safe_user_path(path) {
        Ok(p) => p,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("read: {}", e)));
            return entry;
        }
    };

    // File-content SHA-256 is separate from the pattern hash: the UI surfaces
    // it for debugging ("did this file change on disk?"), while the pattern
    // hash is what drives duplicate detection across formats.
    let file_hash = sha256_hex(&bytes);
    entry.hash_sha256 = Some(file_hash);

    let pattern = match parse_by_format(fmt, &bytes, midi_import_opts) {
        Ok(p) => p,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };

    let content_hash = pattern_hash(&pattern);
    entry.status = FileIngestStatus::Parsed;

    // Duplicate detection - if any existing item has the same content_hash,
    // record the entry as DuplicateSkipped and move on.
    match store.find_item_by_content_hash(&content_hash) {
        Ok(Some(existing)) => {
            entry.status = FileIngestStatus::DuplicateSkipped;
            entry.duplicate_of = Some(existing.item_id);
            return entry;
        }
        Ok(None) => {}
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    }

    let display = file_stem(&path);
    let source_label = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(&display)
        .to_string();

    let mut tags: Vec<String> = Vec::new();
    tags.push(format!("format:{}", fmt));
    // Ensure the Auto-kind tag exists before we attach it.
    if let Err(e) = ensure_auto_tag(store, &format!("format:{}", fmt)) {
        entry.status = FileIngestStatus::Failed;
        entry.error = Some(truncate_err(&e.to_string()));
        return entry;
    }

    let now = store::now_iso();
    let item = LibraryItem {
        item_id: ids::new_id("item"),
        display_name: display,
        source_kind: SourceKind::File,
        source_label,
        source_path: Some(path.to_string_lossy().to_string()),
        created_at: now.clone(),
        updated_at: now,
        tags: tags.clone(),
        favorite: false,
        archived: false,
        slot_key: None,
        snapshot_id: None,
        snapshot_name: None,
        format: Some(fmt.to_string()),
        scale_name: None,
        root_note: None,
        duplicate_status: DuplicateStatus::Unique,
        related_group_count: 0,
        analysis_status: AnalysisStatus::Unknown,
        notes: None,
        content_hash: Some(content_hash.clone()),
    };

    let payload_for_sidecar = match crate::pattern::pattern_to_sysex(&pattern, 0, 0, 0) {
        Ok(sx) if sx.len() >= 3 && sx[3..].len() == 112 => sx[3..].to_vec(),
        Ok(sx) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!(
                "sidecar: unexpected sysex length {}",
                sx.len()
            )));
            return entry;
        }
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("sidecar: {}", e)));
            return entry;
        }
    };

    if let Err(e) = store.write_pattern_bytes(&item.item_id, &payload_for_sidecar) {
        entry.status = FileIngestStatus::Failed;
        entry.error = Some(truncate_err(&format!("sidecar: {}", e)));
        return entry;
    }

    match store.upsert_item(item) {
        Ok(saved) => {
            entry.item_id = Some(saved.item_id.clone());
            entry.status = FileIngestStatus::Imported;
            if let Err(e) = store.add_tag_to_item(&saved.item_id, &format!("format:{}", fmt)) {
                eprintln!(
                    "[ingest] warn: tag attach failed for {}: {}",
                    saved.item_id, e
                );
            }
        }
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
        }
    }

    entry
}
