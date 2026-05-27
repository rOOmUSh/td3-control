use std::path::Path;

use crate::library::model::{FileIndexEntry, FileIngestStatus, SnapshotSlot};
use crate::library::store::LibraryStore;

use super::super::helpers::{dashed_slot_key, pattern_hash, persist_snapshot_slot, truncate_err};
use super::common::{
    create_imported_snapshot, ensure_import_tags, find_or_create_slot_item,
    finish_entry_with_decode_errors, read_bank_file, SlotItemInput,
};

pub(in crate::library::ingest) fn process_sqs(
    store: &LibraryStore,
    path: &Path,
    entry: FileIndexEntry,
) -> FileIndexEntry {
    let (path, bytes, entry) = match read_bank_file(path, entry) {
        Ok(result) => result,
        Err(entry) => return *entry,
    };
    let bank = match crate::formats::sqs::parse_bank(&bytes) {
        Ok(b) => b,
        Err(e) => {
            let mut entry = entry;
            entry.status = FileIngestStatus::Failed;
            entry.error = Some(truncate_err(&e.to_string()));
            return entry;
        }
    };
    let (snapshot, entry) = match create_imported_snapshot(store, &path, "imported-bank", entry) {
        Ok(result) => result,
        Err(entry) => return *entry,
    };
    let mut entry = match ensure_import_tags(store, &["snapshot-origin"], entry) {
        Ok(entry) => entry,
        Err(entry) => return *entry,
    };
    let mut decode_errors = Vec::new();

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
            match process_record(store, &path, &snapshot, rec, &slot_key) {
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

fn process_record(
    store: &LibraryStore,
    path: &Path,
    snapshot: &crate::library::model::Snapshot,
    rec: &crate::formats::sqs::BankRecord,
    slot_key: &str,
) -> Result<String, String> {
    let mut sysex = Vec::with_capacity(3 + rec.payload.len());
    sysex.push(0x78);
    sysex.push(rec.group);
    sysex.push(rec.slot_addr);
    sysex.extend_from_slice(&rec.payload);
    let pattern = crate::pattern::sysex_to_pattern(&sysex).map_err(|e| e.to_string())?;
    let content_hash = pattern_hash(&pattern);

    find_or_create_slot_item(
        store,
        SlotItemInput {
            path,
            snapshot,
            slot_key,
            payload: &rec.payload,
            content_hash: &content_hash,
            format_name: "sqs",
            tags: &["snapshot-origin"],
        },
    )
}
