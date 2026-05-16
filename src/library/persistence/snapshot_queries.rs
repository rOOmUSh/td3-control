use super::transactions::*;
use super::*;

pub fn list_snapshots(path: &Path) -> Result<Vec<Snapshot>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows(&conn, "SELECT json FROM snapshots ORDER BY position")
}

pub fn get_snapshot(path: &Path, id: &str) -> Result<Option<Snapshot>, Td3Error> {
    if id.is_empty() {
        return Ok(None);
    }
    let conn = open_partial_connection(path)?;
    load_one_json_row(
        &conn,
        "SELECT json FROM snapshots WHERE snapshot_id = ?1 LIMIT 1",
        vec![Value::Text(id.to_string())],
    )
}

pub fn list_snapshot_slots(path: &Path, snapshot_id: &str) -> Result<Vec<SnapshotSlot>, Td3Error> {
    let conn = open_partial_connection(path)?;
    let stored: Vec<SnapshotSlot> = load_json_rows_with_params(
        &conn,
        "SELECT json FROM snapshot_slots WHERE snapshot_id = ?1 ORDER BY position",
        vec![Value::Text(snapshot_id.to_string())],
    )?;
    Ok(pad_snapshot_slots(snapshot_id, &stored))
}

pub fn snapshot_exists_with_backup_path(path: &Path, backup_path: &str) -> Result<bool, Td3Error> {
    if backup_path.is_empty() {
        return Ok(false);
    }
    let conn = open_partial_connection(path)?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM snapshots WHERE json_extract(json, '$.backup_path') = ?1",
            params![backup_path],
            |row| row.get(0),
        )
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: sqlite snapshot backup_path lookup '{}': {}",
                backup_path, e
            ))
        })?;
    Ok(count > 0)
}
