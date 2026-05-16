use super::*;

pub fn apply_delete_import_batch(
    path: &Path,
    batch_id: &str,
    items_to_delete: &[String],
    snapshot_ids_to_delete: &[String],
) -> Result<DeleteImportBatchApplyReport, Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite delete_import_batch apply transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;

    let removed_entries = tx
        .execute(
            "DELETE FROM file_index WHERE batch_id = ?1",
            params![batch_id],
        )
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: delete sqlite batch file_index '{}': {}",
                batch_id, e
            ))
        })? as u32;
    tx.execute(
        "DELETE FROM import_batches WHERE batch_id = ?1",
        params![batch_id],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: delete sqlite import_batch '{}': {}",
            batch_id, e
        ))
    })?;

    if !items_to_delete.is_empty() {
        let item_placeholders = sql_placeholders(items_to_delete.len());
        let del_items_set: std::collections::HashSet<&str> =
            items_to_delete.iter().map(|id| id.as_str()).collect();

        let affected_file_index_sql = format!(
            "SELECT position, json
             FROM file_index
             WHERE json_extract(json, '$.item_id') IN ({0})
                OR json_extract(json, '$.duplicate_of') IN ({0})
             ORDER BY position",
            item_placeholders
        );
        let mut entry_params = text_params(items_to_delete);
        entry_params.extend(text_params(items_to_delete));
        let mut touched_entries: Vec<(i64, FileIndexEntry)> =
            load_positioned_json_rows_with_params(&tx, &affected_file_index_sql, entry_params)?;
        for (_, entry) in &mut touched_entries {
            if entry
                .item_id
                .as_deref()
                .is_some_and(|id| del_items_set.contains(id))
            {
                entry.item_id = None;
            }
            if entry
                .duplicate_of
                .as_deref()
                .is_some_and(|id| del_items_set.contains(id))
            {
                entry.duplicate_of = None;
            }
        }

        let mut slot_params = text_params(items_to_delete);
        let affected_slot_sql = if snapshot_ids_to_delete.is_empty() {
            format!(
                "SELECT position, json
                 FROM snapshot_slots
                 WHERE json_extract(json, '$.item_id') IN ({})
                 ORDER BY position",
                item_placeholders
            )
        } else {
            let snapshot_placeholders = sql_placeholders(snapshot_ids_to_delete.len());
            slot_params.extend(text_params(snapshot_ids_to_delete));
            format!(
                "SELECT position, json
                 FROM snapshot_slots
                 WHERE json_extract(json, '$.item_id') IN ({})
                   AND snapshot_id NOT IN ({})
                 ORDER BY position",
                item_placeholders, snapshot_placeholders
            )
        };
        let mut touched_slots: Vec<(i64, SnapshotSlot)> =
            load_positioned_json_rows_with_params(&tx, &affected_slot_sql, slot_params)?;
        for (_, slot) in &mut touched_slots {
            if slot
                .item_id
                .as_deref()
                .is_some_and(|id| del_items_set.contains(id))
            {
                slot.item_id = None;
                slot.empty = true;
            }
        }

        exec_with_text_params(
            &tx,
            &format!(
                "DELETE FROM item_tags WHERE item_id IN ({})",
                item_placeholders
            ),
            items_to_delete,
        )?;
        exec_with_text_params(
            &tx,
            &format!(
                "DELETE FROM pattern_analysis WHERE item_id IN ({})",
                item_placeholders
            ),
            items_to_delete,
        )?;
        {
            let sql = format!(
                "DELETE FROM pattern_relations WHERE from_item_id IN ({0}) OR to_item_id IN ({0})",
                item_placeholders
            );
            let mut params = text_params(items_to_delete);
            params.extend(text_params(items_to_delete));
            tx.execute(&sql, params_from_iter(params.iter()))
                .map_err(|e| Td3Error::Other(format!("library: exec sqlite '{}': {}", sql, e)))?;
        }
        exec_with_text_params(
            &tx,
            &format!("DELETE FROM items WHERE item_id IN ({})", item_placeholders),
            items_to_delete,
        )?;

        for (position, entry) in &touched_entries {
            update_file_index_row_at_position(&tx, *position, entry)?;
        }
        for (position, slot) in &touched_slots {
            update_snapshot_slot_row_at_position(&tx, *position, slot)?;
        }
    }

    if !snapshot_ids_to_delete.is_empty() {
        let snapshot_placeholders = sql_placeholders(snapshot_ids_to_delete.len());
        exec_with_text_params(
            &tx,
            &format!(
                "DELETE FROM snapshot_slots WHERE snapshot_id IN ({})",
                snapshot_placeholders
            ),
            snapshot_ids_to_delete,
        )?;
        exec_with_text_params(
            &tx,
            &format!(
                "DELETE FROM snapshots WHERE snapshot_id IN ({})",
                snapshot_placeholders
            ),
            snapshot_ids_to_delete,
        )?;
    }

    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite delete_import_batch apply transaction: {}",
            e
        ))
    })?;
    Ok(DeleteImportBatchApplyReport { removed_entries })
}
