use std::path::{Path, PathBuf};

use crate::library::ids;
use crate::library::model::{
    AnalysisStatus, DuplicateStatus, FileIndexEntry, FileIngestStatus, LibraryItem, Snapshot,
    SnapshotOrigin, SourceKind,
};
use crate::library::store::{self, LibraryStore};

use super::super::helpers::{ensure_auto_tag, sha256_hex, truncate_err};

pub(super) type EntryResult<T> = Result<T, Box<FileIndexEntry>>;

pub(super) fn read_bank_file(
    path: &Path,
    mut entry: FileIndexEntry,
) -> EntryResult<(PathBuf, Vec<u8>, FileIndexEntry)> {
    let path = match crate::path_safety::require_safe_user_path(path) {
        Ok(p) => p,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return Err(Box::new(entry));
        }
    };
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&format!("read: {}", e)));
            return Err(Box::new(entry));
        }
    };
    entry.hash_sha256 = Some(sha256_hex(&bytes));
    Ok((path, bytes, entry))
}

pub(super) fn create_imported_snapshot(
    store: &LibraryStore,
    path: &Path,
    fallback_name: &str,
    mut entry: FileIndexEntry,
) -> EntryResult<(Snapshot, FileIndexEntry)> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(fallback_name)
        .to_string();

    match store.create_snapshot(name, None, SnapshotOrigin::Imported) {
        Ok(snapshot) => Ok((snapshot, entry)),
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            Err(Box::new(entry))
        }
    }
}

pub(super) fn ensure_import_tags(
    store: &LibraryStore,
    tags: &[&str],
    mut entry: FileIndexEntry,
) -> EntryResult<FileIndexEntry> {
    for tag in tags {
        if let Err(e) = ensure_auto_tag(store, tag) {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return Err(Box::new(entry));
        }
    }
    Ok(entry)
}

pub(super) fn finish_entry_with_decode_errors(
    mut entry: FileIndexEntry,
    decode_errors: &[String],
) -> FileIndexEntry {
    if !decode_errors.is_empty() {
        entry.status = FileIngestStatus::Failed;
        let joined = decode_errors.join("; ");
        entry.error = Some(truncate_err(&format!(
            "{} slot decode error(s): {}",
            decode_errors.len(),
            joined
        )));
    } else {
        entry.status = FileIngestStatus::Imported;
    }

    entry
}

pub(super) fn find_or_create_slot_item(
    store: &LibraryStore,
    input: SlotItemInput<'_>,
) -> Result<String, String> {
    let reuse = store
        .find_item_by_content_hash(input.content_hash)
        .map_err(|e| format!("lookup: {}", e))?;

    if let Some(existing) = reuse {
        return Ok(existing.item_id);
    }

    let now = store::now_iso();
    let tag_values = input.tags.iter().map(|tag| (*tag).to_string()).collect();
    let new_item = LibraryItem {
        item_id: ids::new_id("item"),
        display_name: input.slot_key.to_string(),
        source_kind: SourceKind::SnapshotSlot,
        source_label: format!(
            "{} @ {}",
            input
                .path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(""),
            input.slot_key
        ),
        source_path: Some(input.path.to_string_lossy().to_string()),
        created_at: now.clone(),
        updated_at: now,
        tags: tag_values,
        favorite: false,
        archived: false,
        slot_key: Some(input.slot_key.to_string()),
        snapshot_id: Some(input.snapshot.snapshot_id.clone()),
        snapshot_name: Some(input.snapshot.name.clone()),
        format: Some(input.format_name.to_string()),
        scale_name: None,
        root_note: None,
        duplicate_status: DuplicateStatus::Unique,
        related_group_count: 0,
        analysis_status: AnalysisStatus::Unknown,
        notes: None,
        content_hash: Some(input.content_hash.to_string()),
    };
    let new_item_id = new_item.item_id.clone();
    store
        .write_pattern_bytes(&new_item_id, input.payload)
        .map_err(|e| format!("sidecar: {}", e))?;

    let saved = store
        .upsert_item(new_item)
        .map_err(|e| format!("upsert: {}", e))?;
    for tag in input.tags {
        if let Err(e) = store.add_tag_to_item(&saved.item_id, tag) {
            eprintln!(
                "[ingest] warn: tag attach failed for {}: {}",
                saved.item_id, e
            );
        }
    }
    Ok(saved.item_id)
}

pub(super) struct SlotItemInput<'a> {
    pub path: &'a Path,
    pub snapshot: &'a Snapshot,
    pub slot_key: &'a str,
    pub payload: &'a [u8],
    pub content_hash: &'a str,
    pub format_name: &'a str,
    pub tags: &'a [&'a str],
}
