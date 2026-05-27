use super::*;

pub(in crate::library::persistence) fn write_snapshots(
    conn: &Connection,
    snapshots: &[Snapshot],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_SNAPSHOTS)?;
    let mut stmt = conn
        .prepare("INSERT INTO snapshots (position, snapshot_id, json) VALUES (?1, ?2, ?3)")
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite snapshots insert: {}", e)))?;
    for (idx, snapshot) in snapshots.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            snapshot.snapshot_id.as_str(),
            to_json(snapshot)?
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite snapshot '{}': {}",
                snapshot.snapshot_id, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn upsert_snapshot_row(
    conn: &Connection,
    snapshot: &Snapshot,
) -> Result<(), Td3Error> {
    let position =
        existing_position_by_text_key(conn, TABLE_SNAPSHOTS, "snapshot_id", &snapshot.snapshot_id)?
            .unwrap_or(next_position(conn, TABLE_SNAPSHOTS)?);
    conn.execute(
        "INSERT OR REPLACE INTO snapshots (position, snapshot_id, json) VALUES (?1, ?2, ?3)",
        params![position, snapshot.snapshot_id.as_str(), to_json(snapshot)?],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: upsert sqlite snapshot '{}': {}",
            snapshot.snapshot_id, e
        ))
    })?;
    Ok(())
}

pub(in crate::library::persistence) fn write_snapshot_slots(
    conn: &Connection,
    slots: &[SnapshotSlot],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_SNAPSHOT_SLOTS)?;
    let mut stmt = conn
        .prepare("INSERT INTO snapshot_slots (position, snapshot_id, slot_key, json) VALUES (?1, ?2, ?3, ?4)")
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite snapshot_slots insert: {}", e)))?;
    for (idx, slot) in slots.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            slot.snapshot_id.as_str(),
            slot.slot_key.as_str(),
            to_json(slot)?,
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite snapshot slot '{}:{}': {}",
                slot.snapshot_id, slot.slot_key, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn upsert_snapshot_slot_row(
    conn: &Connection,
    slot: &SnapshotSlot,
) -> Result<(), Td3Error> {
    let position = conn
        .query_row(
            "SELECT position FROM snapshot_slots WHERE snapshot_id = ?1 AND slot_key = ?2",
            params![slot.snapshot_id.as_str(), slot.slot_key.as_str()],
            |row| row.get::<_, i64>(0),
        )
        .ok()
        .unwrap_or(next_position(conn, TABLE_SNAPSHOT_SLOTS)?);
    conn.execute(
        "INSERT OR REPLACE INTO snapshot_slots (position, snapshot_id, slot_key, json) VALUES (?1, ?2, ?3, ?4)",
        params![position, slot.snapshot_id.as_str(), slot.slot_key.as_str(), to_json(slot)?,],
    )
    .map_err(|e| {
        Td3Error::Other(format!("library: upsert sqlite snapshot slot '{}:{}': {}", slot.snapshot_id, slot.slot_key, e))
    })?;
    Ok(())
}

pub(in crate::library::persistence) fn update_snapshot_slot_row_at_position(
    conn: &Connection,
    position: i64,
    slot: &SnapshotSlot,
) -> Result<(), Td3Error> {
    conn.execute(
        "INSERT OR REPLACE INTO snapshot_slots (position, snapshot_id, slot_key, json) VALUES (?1, ?2, ?3, ?4)",
        params![position, slot.snapshot_id.as_str(), slot.slot_key.as_str(), to_json(slot)?,],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: update sqlite snapshot slot '{}:{}' at position {}: {}",
            slot.snapshot_id, slot.slot_key, position, e
        ))
    })?;
    Ok(())
}
