use super::*;

pub(in crate::library::persistence) fn write_import_batches(
    conn: &Connection,
    import_batches: &[ImportBatch],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_IMPORT_BATCHES)?;
    let mut stmt = conn
        .prepare("INSERT INTO import_batches (position, batch_id, json) VALUES (?1, ?2, ?3)")
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: prepare sqlite import_batches insert: {}",
                e
            ))
        })?;
    for (idx, batch) in import_batches.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            batch.batch_id.as_str(),
            to_json(batch)?
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite import_batch '{}': {}",
                batch.batch_id, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn upsert_import_batch_row(
    conn: &Connection,
    batch: &ImportBatch,
) -> Result<(), Td3Error> {
    let position =
        existing_position_by_text_key(conn, TABLE_IMPORT_BATCHES, "batch_id", &batch.batch_id)?
            .unwrap_or(next_position(conn, TABLE_IMPORT_BATCHES)?);
    conn.execute(
        "INSERT OR REPLACE INTO import_batches (position, batch_id, json) VALUES (?1, ?2, ?3)",
        params![position, batch.batch_id.as_str(), to_json(batch)?],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: upsert sqlite import_batch '{}': {}",
            batch.batch_id, e
        ))
    })?;
    Ok(())
}
