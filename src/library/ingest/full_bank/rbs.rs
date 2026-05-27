use std::path::Path;

use crate::library::model::{FileIndexEntry, FileIngestStatus, SnapshotSlot};
use crate::library::store::LibraryStore;

use super::super::helpers::{dashed_slot_key, pattern_hash, persist_snapshot_slot, truncate_err};
use super::common::{
    create_imported_snapshot, ensure_import_tags, find_or_create_slot_item,
    finish_entry_with_decode_errors, read_bank_file, SlotItemInput,
};

pub(in crate::library::ingest) fn process_rbs(
    store: &LibraryStore,
    path: &Path,
    entry: FileIndexEntry,
) -> FileIndexEntry {
    let (path, bytes, entry) = match read_bank_file(path, entry) {
        Ok(result) => result,
        Err(entry) => return *entry,
    };
    let song = match crate::formats::rbs::RbsSong::parse(&bytes) {
        Ok(s) => s,
        Err(e) => {
            let mut entry = entry;
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };
    let (snapshot, entry) = match create_imported_snapshot(store, &path, "imported-rbs", entry) {
        Ok(result) => result,
        Err(entry) => return *entry,
    };
    let mut entry = match ensure_import_tags(store, &["snapshot-origin", "format:rbs"], entry) {
        Ok(entry) => entry,
        Err(entry) => return *entry,
    };
    let mut decode_errors = Vec::new();

    for (flat, pattern) in song.patterns().iter().enumerate() {
        let slot_key = rbs_slot_key(flat);
        let empty = song.has_padding_signature(flat)
            || pattern
                .step
                .iter()
                .all(|s| s.time == crate::step::Time::Rest);
        let mut slot_row = SnapshotSlot {
            snapshot_id: snapshot.snapshot_id.clone(),
            slot_key: slot_key.clone(),
            item_id: None,
            empty,
            display_name: Some(slot_key.clone()),
        };

        if !empty {
            match process_pattern(store, &path, &snapshot, pattern, &slot_key) {
                Ok(item_id) => slot_row.item_id = Some(item_id),
                Err(error) => {
                    decode_errors.push(format!("{}: {}", slot_key, error));
                    if !persist_snapshot_slot(store, &mut entry, slot_row) {
                        return entry;
                    }
                    continue;
                }
            }
        }
        if !persist_snapshot_slot(store, &mut entry, slot_row) {
            return entry;
        }
    }

    finish_entry_with_decode_errors(entry, &decode_errors)
}

fn rbs_slot_key(flat: usize) -> String {
    let device = flat / crate::formats::rbs::SLOTS_PER_DEVICE;
    let within_device = flat % crate::formats::rbs::SLOTS_PER_DEVICE;
    let group = (within_device / crate::formats::rbs::SLOTS_PER_GROUP) as u8;
    let slot = (within_device % crate::formats::rbs::SLOTS_PER_GROUP) as u8;
    let slot_addr = slot | ((device as u8) << 3);
    dashed_slot_key(group, slot_addr)
}

fn process_pattern(
    store: &LibraryStore,
    path: &Path,
    snapshot: &crate::library::model::Snapshot,
    pattern: &crate::pattern::Pattern,
    slot_key: &str,
) -> Result<String, String> {
    let sysex = crate::pattern::pattern_to_sysex(pattern, 0, 0, 0).map_err(|e| e.to_string())?;
    if sysex.len() < 3 || sysex[3..].len() != 112 {
        return Err("unexpected sysex length".to_string());
    }
    let payload = sysex[3..].to_vec();
    let content_hash = pattern_hash(pattern);

    find_or_create_slot_item(
        store,
        SlotItemInput {
            path,
            snapshot,
            slot_key,
            payload: &payload,
            content_hash: &content_hash,
            format_name: "rbs",
            tags: &["snapshot-origin", "format:rbs"],
        },
    )
}
