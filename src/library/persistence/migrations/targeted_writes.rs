use super::*;

pub fn append_file_index_entry(path: &Path, entry: &FileIndexEntry) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite append file_index transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    insert_file_index_row(&tx, next_position(&tx, TABLE_FILE_INDEX)?, entry)?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite append file_index transaction: {}",
            e
        ))
    })
}

pub fn replace_file_index_entry(path: &Path, entry: &FileIndexEntry) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite replace file_index transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    let existing_position = existing_file_index_position(&tx, entry)?;
    delete_matching_file_index_rows(&tx, entry)?;
    insert_file_index_row(
        &tx,
        existing_position.unwrap_or(next_position(&tx, TABLE_FILE_INDEX)?),
        entry,
    )?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite replace file_index transaction: {}",
            e
        ))
    })
}

pub fn upsert_import_batch(path: &Path, batch: &ImportBatch) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite upsert import_batch transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_import_batch_row(&tx, batch)?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite upsert import_batch transaction: {}",
            e
        ))
    })
}

pub fn append_backup_snapshot_bundle(
    path: &Path,
    snapshot: &Snapshot,
    slots: &[SnapshotSlot],
    items: &[LibraryItem],
) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite backup snapshot bundle transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_snapshot_row(&tx, snapshot)?;
    for slot in slots {
        upsert_snapshot_slot_row(&tx, slot)?;
    }
    for item in items {
        upsert_item_row(&tx, item)?;
    }
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite backup snapshot bundle transaction: {}",
            e
        ))
    })
}
