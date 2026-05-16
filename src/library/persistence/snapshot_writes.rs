use super::transactions::*;
use super::*;

pub fn upsert_snapshot(path: &Path, snapshot: &Snapshot) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite upsert snapshot transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_snapshot_row(&tx, snapshot)?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite upsert snapshot transaction: {}",
            e
        ))
    })
}

pub fn upsert_snapshot_slot(path: &Path, slot: &SnapshotSlot) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite upsert snapshot_slot transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_snapshot_slot_row(&tx, slot)?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite upsert snapshot_slot transaction: {}",
            e
        ))
    })
}

/// Move the snapshot-slot row stored at `from_key` to `to_key`. When `to_key`
/// is currently empty the row is renamed in place; when `to_key` is occupied
/// the two rows are swapped (each row's `slot_key` field - both column and
/// JSON payload - is updated to its new home).
///
/// The source row MUST exist; an empty `from_key` is rejected as a logic error.
/// `LibraryItem`s referenced by either slot are left alone - their `slot_key`
/// records the catalog entry's *origin*, which is intentionally immutable.
///
/// Implementation note: the table has `UNIQUE(snapshot_id, slot_key)`, so the
/// swap path can't simply UPDATE both rows in sequence (the second update
/// would collide with the first). We instead delete both rows and re-insert
/// them with swapped keys inside one transaction, which keeps the schema's
/// uniqueness invariant intact at every commit boundary.
pub fn move_snapshot_slot(
    path: &Path,
    snapshot_id: &str,
    from_key: &str,
    to_key: &str,
) -> Result<bool, Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite move_snapshot_slot transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;

    let from_slot = read_snapshot_slot_row(&tx, snapshot_id, from_key)?.ok_or_else(|| {
        Td3Error::Other(format!("source slot '{}' is empty or missing", from_key))
    })?;
    let to_slot = read_snapshot_slot_row(&tx, snapshot_id, to_key)?;
    let swapped = to_slot.is_some();

    // Drop both rows so the upserts below can't collide on the unique
    // (snapshot_id, slot_key) index. Missing rows are no-ops.
    tx.execute(
        "DELETE FROM snapshot_slots WHERE snapshot_id = ?1 AND slot_key IN (?2, ?3)",
        params![snapshot_id, from_key, to_key],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: clear move_snapshot_slot rows '{}': {}",
            snapshot_id, e
        ))
    })?;

    let new_at_to = SnapshotSlot {
        slot_key: to_key.to_string(),
        ..from_slot
    };
    upsert_snapshot_slot_row(&tx, &new_at_to)?;
    if let Some(ts) = to_slot {
        let new_at_from = SnapshotSlot {
            slot_key: from_key.to_string(),
            ..ts
        };
        upsert_snapshot_slot_row(&tx, &new_at_from)?;
    }

    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite move_snapshot_slot transaction: {}",
            e
        ))
    })?;
    Ok(swapped)
}

fn read_snapshot_slot_row(
    conn: &Connection,
    snapshot_id: &str,
    slot_key: &str,
) -> Result<Option<SnapshotSlot>, Td3Error> {
    let row: Option<String> = conn
        .query_row(
            "SELECT json FROM snapshot_slots WHERE snapshot_id = ?1 AND slot_key = ?2",
            params![snapshot_id, slot_key],
            |row| row.get::<_, String>(0),
        )
        .ok();
    match row {
        None => Ok(None),
        Some(text) => {
            let slot: SnapshotSlot = serde_json::from_str(&text).map_err(|e| {
                Td3Error::Other(format!(
                    "library: decode snapshot slot '{}:{}': {}",
                    snapshot_id, slot_key, e
                ))
            })?;
            Ok(Some(slot))
        }
    }
}

/// Delete the rows in `snapshot_slots` matching `(snapshot_id, slot_key)` for
/// every key in `slot_keys`. Missing rows are ignored - caller decides whether
/// "no-op delete" is an error. Returns the number of rows actually removed.
pub fn delete_snapshot_slots(
    path: &Path,
    snapshot_id: &str,
    slot_keys: &[String],
) -> Result<usize, Td3Error> {
    if slot_keys.is_empty() {
        return Ok(0);
    }
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite delete_snapshot_slots transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    let placeholders = sql_placeholders(slot_keys.len());
    let sql = format!(
        "DELETE FROM snapshot_slots WHERE snapshot_id = ?1 AND slot_key IN ({})",
        placeholders,
    );
    let mut params_vec: Vec<Value> = Vec::with_capacity(slot_keys.len() + 1);
    params_vec.push(Value::Text(snapshot_id.to_string()));
    for k in slot_keys {
        params_vec.push(Value::Text(k.clone()));
    }
    let removed = tx
        .execute(&sql, params_from_iter(params_vec.iter()))
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: delete_snapshot_slots '{}': {}",
                snapshot_id, e
            ))
        })?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite delete_snapshot_slots transaction: {}",
            e
        ))
    })?;
    Ok(removed)
}
