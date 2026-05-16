use super::*;

pub fn plan_delete_import_batch(
    path: &Path,
    batch_id: &str,
) -> Result<DeleteImportBatchPlan, Td3Error> {
    let conn = open_partial_connection(path)?;

    let batch_existed =
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM import_batches WHERE batch_id = ?1)",
            params![batch_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: sqlite import_batch exists '{}': {}",
                batch_id, e
            ))
        })? > 0;

    let batch_paths = load_text_rows_with_params(
        &conn,
        "SELECT DISTINCT path FROM file_index WHERE batch_id = ?1 ORDER BY path",
        vec![Value::Text(batch_id.to_string())],
    )?;
    let seed_item_ids = load_text_rows_with_params(
        &conn,
        "SELECT DISTINCT json_extract(json, '$.item_id') FROM file_index WHERE batch_id = ?1 AND json_extract(json, '$.item_id') IS NOT NULL ORDER BY 1",
        vec![Value::Text(batch_id.to_string())],
    )?;

    let mut candidate_item_ids: std::collections::BTreeSet<String> =
        seed_item_ids.into_iter().collect();
    if !batch_paths.is_empty() {
        let path_placeholders = sql_placeholders(batch_paths.len());
        let sql = format!(
            "SELECT DISTINCT item_id FROM items WHERE source_path IN ({}) ORDER BY item_id",
            path_placeholders
        );
        for item_id in load_text_rows_with_params(&conn, &sql, text_params(&batch_paths))? {
            candidate_item_ids.insert(item_id);
        }
    }

    let items_to_delete = if candidate_item_ids.is_empty() {
        Vec::new()
    } else {
        let candidate_values: Vec<String> = candidate_item_ids.into_iter().collect();
        let candidate_placeholders = sql_placeholders(candidate_values.len());
        let sql = format!(
            "SELECT i.item_id
             FROM items i
             WHERE i.item_id IN ({})
               AND NOT EXISTS (
                   SELECT 1
                   FROM file_index fi
                   WHERE COALESCE(fi.batch_id, '') != ?
                     AND (
                         json_extract(fi.json, '$.item_id') = i.item_id
                         OR json_extract(fi.json, '$.duplicate_of') = i.item_id
                     )
               )
             ORDER BY i.position",
            candidate_placeholders
        );
        let mut params = text_params(&candidate_values);
        params.push(Value::Text(batch_id.to_string()));
        load_text_rows_with_params(&conn, &sql, params)?
    };

    let snapshots_to_delete = if batch_paths.is_empty() {
        Vec::new()
    } else {
        let path_placeholders = sql_placeholders(batch_paths.len());
        let sql = format!(
            "SELECT s.snapshot_id
             FROM snapshots s
             JOIN snapshot_slots ss ON ss.snapshot_id = s.snapshot_id
             LEFT JOIN items i ON i.item_id = json_extract(ss.json, '$.item_id')
             WHERE json_extract(s.json, '$.origin') = 'imported'
             GROUP BY s.snapshot_id
             HAVING SUM(CASE WHEN json_extract(ss.json, '$.item_id') IS NOT NULL THEN 1 ELSE 0 END) > 0
                AND SUM(
                    CASE
                        WHEN json_extract(ss.json, '$.item_id') IS NOT NULL
                         AND i.source_path IN ({})
                        THEN 1 ELSE 0
                    END
                ) = SUM(CASE WHEN json_extract(ss.json, '$.item_id') IS NOT NULL THEN 1 ELSE 0 END)
             ORDER BY MIN(s.position)",
            path_placeholders
        );
        load_text_rows_with_params(&conn, &sql, text_params(&batch_paths))?
    };

    let batch_file_stems: Vec<String> = batch_paths
        .iter()
        .filter_map(|path| {
            Path::new(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(|stem| stem.to_string())
        })
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let orphan_snapshot_ids = if batch_file_stems.is_empty() {
        Vec::new()
    } else {
        let stem_placeholders = sql_placeholders(batch_file_stems.len());
        let sql = format!(
            "SELECT snapshot_id
             FROM snapshots
             WHERE json_extract(json, '$.origin') = 'imported'
               AND json_extract(json, '$.name') IN ({})
             ORDER BY position",
            stem_placeholders
        );
        load_text_rows_with_params(&conn, &sql, text_params(&batch_file_stems))?
    };

    Ok(DeleteImportBatchPlan {
        batch_existed,
        batch_paths,
        items_to_delete,
        snapshots_to_delete,
        orphan_snapshot_ids,
    })
}
