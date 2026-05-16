use std::path::Path;

use crate::library::ids;
use crate::library::model::{
    AnalysisStatus, DuplicateStatus, FileIndexEntry, FileIngestStatus, LibraryItem, SnapshotOrigin,
    SnapshotSlot, SourceKind,
};
use crate::library::store::{self, LibraryStore};

use super::helpers::{
    dashed_slot_key, ensure_auto_tag, pattern_hash, persist_snapshot_slot, sha256_hex, truncate_err,
};

pub(super) fn process_sqs(
    store: &LibraryStore,
    path: &Path,
    mut entry: FileIndexEntry,
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
    entry.hash_sha256 = Some(sha256_hex(&bytes));

    let bank = match crate::formats::sqs::parse_bank(&bytes) {
        Ok(b) => b,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };

    // Build the snapshot shell first so we have an ID to link slots + items to.
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-bank")
        .to_string();

    let snapshot = match store.create_snapshot(name, None, SnapshotOrigin::Imported) {
        Ok(s) => s,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };

    // Iterate the 64 records. For each non-silent one, decode the payload to a
    // Pattern so we can hash + maybe-dedupe; silent slots just get an empty
    // placeholder row.
    let mut decode_errors: Vec<String> = Vec::new();

    if let Err(e) = ensure_auto_tag(store, "snapshot-origin") {
        // Ensure-tag failures are fatal for item creation, surface them.
        entry.status = FileIngestStatus::Failed;
        entry.error = Some(truncate_err(&e.to_string()));
        return entry;
    }

    for rec in bank.records.iter() {
        let slot_key = dashed_slot_key(rec.group, rec.slot_addr);
        let empty = crate::formats::sqs::is_silent(&rec.payload);
        let mut slot_row = SnapshotSlot {
            snapshot_id: snapshot.snapshot_id.clone(),
            slot_key: slot_key.clone(),
            item_id: None,
            empty,
            display_name: Some(slot_key.clone()),
        };

        if !empty {
            // Reconstruct a SysEx-shaped body so we can reuse `sysex_to_pattern`.
            // The 112-byte payload lacks the 3-byte header (kind + group + slot)
            // so synthesise those to keep the decoder happy.
            let mut sysex = Vec::with_capacity(3 + rec.payload.len());
            sysex.push(0x78);
            sysex.push(rec.group);
            sysex.push(rec.slot_addr);
            sysex.extend_from_slice(&rec.payload);
            let pattern = match crate::pattern::sysex_to_pattern(&sysex) {
                Ok(p) => p,
                Err(e) => {
                    decode_errors.push(format!("{}: {}", slot_key, e));
                    // Still write the slot so the grid stays 64-wide.
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }
            };
            let content_hash = pattern_hash(&pattern);

            let reuse = match store.find_item_by_content_hash(&content_hash) {
                Ok(x) => x,
                Err(e) => {
                    decode_errors.push(format!("{}: lookup: {}", slot_key, e));
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }
            };

            let item_id = if let Some(existing) = reuse {
                existing.item_id
            } else {
                let now = store::now_iso();
                let new_item = LibraryItem {
                    item_id: ids::new_id("item"),
                    display_name: slot_key.clone(),
                    source_kind: SourceKind::SnapshotSlot,
                    source_label: format!(
                        "{} @ {}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                        slot_key
                    ),
                    source_path: Some(path.to_string_lossy().to_string()),
                    created_at: now.clone(),
                    updated_at: now,
                    tags: vec!["snapshot-origin".to_string()],
                    favorite: false,
                    archived: false,
                    slot_key: Some(slot_key.clone()),
                    snapshot_id: Some(snapshot.snapshot_id.clone()),
                    snapshot_name: Some(snapshot.name.clone()),
                    format: Some("sqs".to_string()),
                    scale_name: None,
                    root_note: None,
                    duplicate_status: DuplicateStatus::Unique,
                    related_group_count: 0,
                    analysis_status: AnalysisStatus::Unknown,
                    notes: None,
                    content_hash: Some(content_hash.clone()),
                };
                let new_item_id = new_item.item_id.clone();
                if let Err(e) = store.write_pattern_bytes(&new_item_id, &rec.payload) {
                    decode_errors.push(format!("{}: sidecar: {}", slot_key, e));
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }

                match store.upsert_item(new_item) {
                    Ok(saved) => {
                        if let Err(e) = store.add_tag_to_item(&saved.item_id, "snapshot-origin") {
                            eprintln!(
                                "[ingest] warn: tag attach failed for {}: {}",
                                saved.item_id, e
                            );
                        }
                        saved.item_id
                    }
                    Err(e) => {
                        decode_errors.push(format!("{}: upsert: {}", slot_key, e));
                        if !persist_snapshot_slot(store, &mut entry, slot_row) {
                            return entry;
                        }
                        continue;
                    }
                }
            };

            slot_row.item_id = Some(item_id);
        }
        if !persist_snapshot_slot(store, &mut entry, slot_row) {
            return entry;
        }
    }

    // If any slot failed to decode, flag the whole entry Failed but keep the
    // partially-imported items (Idea is to keep partially imported
    // items already persisted). Use a short summary.
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

pub(super) fn process_rbs(
    store: &LibraryStore,
    path: &Path,
    mut entry: FileIndexEntry,
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
    entry.hash_sha256 = Some(sha256_hex(&bytes));

    let song = match crate::formats::rbs::RbsSong::parse(&bytes) {
        Ok(s) => s,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };

    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported-rbs")
        .to_string();

    let snapshot = match store.create_snapshot(name, None, SnapshotOrigin::Imported) {
        Ok(s) => s,
        Err(e) => {
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };

    let mut decode_errors: Vec<String> = Vec::new();

    if let Err(e) = ensure_auto_tag(store, "snapshot-origin") {
        entry.status = FileIngestStatus::Failed;
        entry.error = Some(truncate_err(&e.to_string()));
        return entry;
    }
    if let Err(e) = ensure_auto_tag(store, "format:rbs") {
        entry.status = FileIngestStatus::Failed;
        entry.error = Some(truncate_err(&e.to_string()));
        return entry;
    }

    let patterns = song.patterns();
    for (flat, pattern) in patterns.iter().enumerate() {
        let device = flat / crate::formats::rbs::SLOTS_PER_DEVICE;
        let within_device = flat % crate::formats::rbs::SLOTS_PER_DEVICE;
        let group = (within_device / crate::formats::rbs::SLOTS_PER_GROUP) as u8;
        let slot = (within_device % crate::formats::rbs::SLOTS_PER_GROUP) as u8;
        let slot_addr = slot | ((device as u8) << 3);
        let slot_key = dashed_slot_key(group, slot_addr);

        let is_padding = song.has_padding_signature(flat);
        let all_rest = pattern
            .step
            .iter()
            .all(|s| s.time == crate::step::Time::Rest);
        let empty = is_padding || all_rest;

        let mut slot_row = SnapshotSlot {
            snapshot_id: snapshot.snapshot_id.clone(),
            slot_key: slot_key.clone(),
            item_id: None,
            empty,
            display_name: Some(slot_key.clone()),
        };

        if !empty {
            let sysex = match crate::pattern::pattern_to_sysex(pattern, 0, 0, 0) {
                Ok(s) => s,
                Err(e) => {
                    decode_errors.push(format!("{}: {}", slot_key, e));
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }
            };
            if sysex.len() < 3 || sysex[3..].len() != 112 {
                decode_errors.push(format!("{}: unexpected sysex length", slot_key));
                if !persist_snapshot_slot(store, &mut entry, slot_row) {
                    return entry;
                }
                continue;
            }
            let payload = sysex[3..].to_vec();
            let content_hash = pattern_hash(pattern);

            let reuse = match store.find_item_by_content_hash(&content_hash) {
                Ok(x) => x,
                Err(e) => {
                    decode_errors.push(format!("{}: lookup: {}", slot_key, e));
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }
            };

            let item_id = if let Some(existing) = reuse {
                existing.item_id
            } else {
                let now = store::now_iso();
                let new_item = LibraryItem {
                    item_id: ids::new_id("item"),
                    display_name: slot_key.clone(),
                    source_kind: SourceKind::SnapshotSlot,
                    source_label: format!(
                        "{} @ {}",
                        path.file_name().and_then(|n| n.to_str()).unwrap_or(""),
                        slot_key
                    ),
                    source_path: Some(path.to_string_lossy().to_string()),
                    created_at: now.clone(),
                    updated_at: now,
                    tags: vec!["snapshot-origin".to_string(), "format:rbs".to_string()],
                    favorite: false,
                    archived: false,
                    slot_key: Some(slot_key.clone()),
                    snapshot_id: Some(snapshot.snapshot_id.clone()),
                    snapshot_name: Some(snapshot.name.clone()),
                    format: Some("rbs".to_string()),
                    scale_name: None,
                    root_note: None,
                    duplicate_status: DuplicateStatus::Unique,
                    related_group_count: 0,
                    analysis_status: AnalysisStatus::Unknown,
                    notes: None,
                    content_hash: Some(content_hash.clone()),
                };
                let new_item_id = new_item.item_id.clone();
                if let Err(e) = store.write_pattern_bytes(&new_item_id, &payload) {
                    decode_errors.push(format!("{}: sidecar: {}", slot_key, e));
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }

                match store.upsert_item(new_item) {
                    Ok(saved) => {
                        if let Err(e) = store.add_tag_to_item(&saved.item_id, "snapshot-origin") {
                            eprintln!(
                                "[ingest] warn: tag attach failed for {}: {}",
                                saved.item_id, e
                            );
                        }
                        if let Err(e) = store.add_tag_to_item(&saved.item_id, "format:rbs") {
                            eprintln!(
                                "[ingest] warn: tag attach failed for {}: {}",
                                saved.item_id, e
                            );
                        }
                        saved.item_id
                    }
                    Err(e) => {
                        decode_errors.push(format!("{}: upsert: {}", slot_key, e));
                        if !persist_snapshot_slot(store, &mut entry, slot_row) {
                            return entry;
                        }
                        continue;
                    }
                }
            };

            slot_row.item_id = Some(item_id);
        }
        if !persist_snapshot_slot(store, &mut entry, slot_row) {
            return entry;
        }
    }

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
