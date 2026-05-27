use super::*;

pub(in crate::library::persistence) fn write_file_index(
    conn: &Connection,
    file_index: &[FileIndexEntry],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_FILE_INDEX)?;
    let mut stmt = conn
        .prepare("INSERT INTO file_index (position, batch_id, path, json) VALUES (?1, ?2, ?3, ?4)")
        .map_err(|e| {
            Td3Error::Other(format!("library: prepare sqlite file_index insert: {}", e))
        })?;
    for (idx, entry) in file_index.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            entry.batch_id.as_deref(),
            entry.path.as_str(),
            to_json(entry)?,
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite file_index '{}': {}",
                entry.path, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn existing_file_index_position(
    conn: &Connection,
    entry: &FileIndexEntry,
) -> Result<Option<i64>, Td3Error> {
    match entry.batch_id.as_deref() {
        Some(batch_id) => {
            let mut stmt = conn
                .prepare("SELECT position FROM file_index WHERE batch_id = ?1 AND path = ?2 ORDER BY position LIMIT 1")
                .map_err(|e| Td3Error::Other(format!("library: prepare sqlite file_index position lookup: {}", e)))?;
            let mut rows = stmt
                .query(params![batch_id, entry.path.as_str()])
                .map_err(|e| {
                    Td3Error::Other(format!(
                        "library: query sqlite file_index position lookup: {}",
                        e
                    ))
                })?;
            if let Some(row) = rows.next().map_err(|e| {
                Td3Error::Other(format!(
                    "library: iterate sqlite file_index position lookup: {}",
                    e
                ))
            })? {
                let pos: i64 = row.get(0).map_err(|e| {
                    Td3Error::Other(format!(
                        "library: decode sqlite file_index position lookup: {}",
                        e
                    ))
                })?;
                Ok(Some(pos))
            } else {
                Ok(None)
            }
        }
        None => Ok(None),
    }
}

pub(in crate::library::persistence) fn delete_matching_file_index_rows(
    conn: &Connection,
    entry: &FileIndexEntry,
) -> Result<(), Td3Error> {
    match entry.batch_id.as_deref() {
        Some(batch_id) => conn
            .execute(
                "DELETE FROM file_index WHERE batch_id = ?1 AND path = ?2",
                params![batch_id, entry.path.as_str()],
            )
            .map_err(|e| {
                Td3Error::Other(format!(
                    "library: delete sqlite file_index '{}:{}': {}",
                    batch_id, entry.path, e
                ))
            })?,
        None => conn
            .execute(
                "DELETE FROM file_index WHERE batch_id IS NULL AND path = ?1",
                params![entry.path.as_str()],
            )
            .map_err(|e| {
                Td3Error::Other(format!(
                    "library: delete sqlite file_index '<null>:{}': {}",
                    entry.path, e
                ))
            })?,
    };
    Ok(())
}

pub(in crate::library::persistence) fn insert_file_index_row(
    conn: &Connection,
    position: i64,
    entry: &FileIndexEntry,
) -> Result<(), Td3Error> {
    conn.execute(
        "INSERT INTO file_index (position, batch_id, path, json) VALUES (?1, ?2, ?3, ?4)",
        params![
            position,
            entry.batch_id.as_deref(),
            entry.path.as_str(),
            to_json(entry)?,
        ],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: insert sqlite file_index '{}': {}",
            entry.path, e
        ))
    })?;
    Ok(())
}

pub(in crate::library::persistence) fn update_file_index_row_at_position(
    conn: &Connection,
    position: i64,
    entry: &FileIndexEntry,
) -> Result<(), Td3Error> {
    conn.execute(
        "INSERT OR REPLACE INTO file_index (position, batch_id, path, json) VALUES (?1, ?2, ?3, ?4)",
        params![position, entry.batch_id.as_deref(), entry.path.as_str(), to_json(entry)?,],
    )
    .map_err(|e| {
        Td3Error::Other(format!("library: update sqlite file_index '{}' at position {}: {}", entry.path, position, e))
    })?;
    Ok(())
}
