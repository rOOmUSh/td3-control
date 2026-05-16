use super::*;

pub(in crate::library::persistence) fn save_data(
    conn: &Connection,
    data: &LibraryData,
) -> Result<(), Td3Error> {
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| Td3Error::Other(format!("library: begin sqlite transaction: {}", e)))?;

    write_format_version(&tx)?;
    write_items(&tx, &data.items)?;
    write_snapshots(&tx, &data.snapshots)?;
    write_snapshot_slots(&tx, &data.snapshot_slots)?;
    write_tags(&tx, &data.tags)?;
    write_item_tags(&tx, &data.item_tags)?;
    write_file_index(&tx, &data.file_index)?;
    write_pattern_analysis(&tx, &data.pattern_analysis)?;
    write_pattern_relations(&tx, &data.pattern_relations)?;
    write_import_batches(&tx, &data.import_batches)?;

    tx.commit()
        .map_err(|e| Td3Error::Other(format!("library: commit sqlite transaction: {}", e)))?;
    Ok(())
}

pub(in crate::library::persistence) fn clear_table(
    conn: &Connection,
    table: &str,
) -> Result<(), Td3Error> {
    conn.execute(&format!("DELETE FROM {}", table), [])
        .map_err(|e| Td3Error::Other(format!("library: clear sqlite table '{}': {}", table, e)))?;
    Ok(())
}

pub(in crate::library::persistence) fn next_position(
    conn: &Connection,
    table: &str,
) -> Result<i64, Td3Error> {
    conn.query_row(
        &format!("SELECT COALESCE(MAX(position), -1) + 1 FROM {}", table),
        [],
        |row| row.get(0),
    )
    .map_err(|e| Td3Error::Other(format!("library: next sqlite position '{}': {}", table, e)))
}

pub(in crate::library::persistence) fn sql_placeholders(count: usize) -> String {
    std::iter::repeat_n("?", count)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(in crate::library::persistence) fn text_params(values: &[String]) -> Vec<Value> {
    values.iter().cloned().map(Value::Text).collect()
}

pub(in crate::library::persistence) fn exec_with_text_params(
    conn: &Connection,
    sql: &str,
    values: &[String],
) -> Result<usize, Td3Error> {
    let params = text_params(values);
    conn.execute(sql, params_from_iter(params.iter()))
        .map_err(|e| Td3Error::Other(format!("library: exec sqlite '{}': {}", sql, e)))
}

pub(in crate::library::persistence) fn existing_position_by_text_key(
    conn: &Connection,
    table: &str,
    key_column: &str,
    key_value: &str,
) -> Result<Option<i64>, Td3Error> {
    let sql = format!("SELECT position FROM {} WHERE {} = ?1", table, key_column);
    let mut stmt = conn.prepare(&sql).map_err(|e| {
        Td3Error::Other(format!(
            "library: prepare sqlite position lookup '{}': {}",
            table, e
        ))
    })?;
    let mut rows = stmt.query(params![key_value]).map_err(|e| {
        Td3Error::Other(format!(
            "library: query sqlite position lookup '{}': {}",
            table, e
        ))
    })?;
    if let Some(row) = rows.next().map_err(|e| {
        Td3Error::Other(format!(
            "library: iterate sqlite position lookup '{}': {}",
            table, e
        ))
    })? {
        let pos: i64 = row.get(0).map_err(|e| {
            Td3Error::Other(format!(
                "library: decode sqlite position lookup '{}': {}",
                table, e
            ))
        })?;
        Ok(Some(pos))
    } else {
        Ok(None)
    }
}

pub(in crate::library::persistence) fn write_format_version(
    conn: &Connection,
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_METADATA)?;
    conn.execute(
        "INSERT INTO metadata (key, value_text) VALUES ('format_version', ?1)",
        params![LibraryData::CURRENT_FORMAT_VERSION.to_string()],
    )
    .map_err(|e| Td3Error::Other(format!("library: write sqlite format_version: {}", e)))?;
    Ok(())
}

pub(in crate::library::persistence) fn write_items(
    conn: &Connection,
    items: &[LibraryItem],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_ITEMS)?;
    let mut stmt = conn
        .prepare(ITEM_INSERT_SQL)
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite items insert: {}", e)))?;
    for (idx, item) in items.iter().enumerate() {
        stmt.execute(params_from_iter(
            item_insert_params(idx as i64, item)?.iter(),
        ))
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite item '{}': {}",
                item.item_id, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn upsert_item_row(
    conn: &Connection,
    item: &LibraryItem,
) -> Result<(), Td3Error> {
    let position = existing_position_by_text_key(conn, TABLE_ITEMS, "item_id", &item.item_id)?
        .unwrap_or(next_position(conn, TABLE_ITEMS)?);
    conn.execute(
        ITEM_UPSERT_SQL,
        params_from_iter(item_insert_params(position, item)?.iter()),
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: upsert sqlite item '{}': {}",
            item.item_id, e
        ))
    })?;
    Ok(())
}

const ITEM_INSERT_SQL: &str = "INSERT INTO items (\
    position, item_id, json, display_name, source_kind, source_label, source_path, \
    created_at, updated_at, favorite, archived, slot_key, snapshot_id, format, \
    scale_name, root_note, duplicate_status, analysis_status, notes, content_hash\
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";

const ITEM_UPSERT_SQL: &str = "INSERT OR REPLACE INTO items (\
    position, item_id, json, display_name, source_kind, source_label, source_path, \
    created_at, updated_at, favorite, archived, slot_key, snapshot_id, format, \
    scale_name, root_note, duplicate_status, analysis_status, notes, content_hash\
) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)";

fn item_insert_params(position: i64, item: &LibraryItem) -> Result<Vec<Value>, Td3Error> {
    Ok(vec![
        Value::Integer(position),
        Value::Text(item.item_id.clone()),
        Value::Text(to_json(item)?),
        Value::Text(item.display_name.clone()),
        Value::Text(source_kind_text(item.source_kind)),
        Value::Text(item.source_label.clone()),
        opt_text(&item.source_path),
        Value::Text(item.created_at.clone()),
        Value::Text(item.updated_at.clone()),
        Value::Integer(if item.favorite { 1 } else { 0 }),
        Value::Integer(if item.archived { 1 } else { 0 }),
        opt_text(&item.slot_key),
        opt_text(&item.snapshot_id),
        opt_text(&item.format),
        opt_text(&item.scale_name),
        opt_text(&item.root_note),
        Value::Text(duplicate_status_text(item.duplicate_status)),
        Value::Text(analysis_status_text(item.analysis_status)),
        opt_text(&item.notes),
        opt_text(&item.content_hash),
    ])
}

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

pub(in crate::library::persistence) fn write_tags(
    conn: &Connection,
    tags: &[Tag],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_TAGS)?;
    let mut stmt = conn
        .prepare("INSERT INTO tags (position, tag_id, label, json) VALUES (?1, ?2, ?3, ?4)")
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite tags insert: {}", e)))?;
    for (idx, tag) in tags.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            tag.tag_id.as_str(),
            tag.label.as_str(),
            to_json(tag)?,
        ])
        .map_err(|e| {
            Td3Error::Other(format!("library: insert sqlite tag '{}': {}", tag.label, e))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn upsert_tag_row(
    conn: &Connection,
    tag: &Tag,
) -> Result<(), Td3Error> {
    let position = existing_position_by_text_key(conn, TABLE_TAGS, "tag_id", &tag.tag_id)?
        .unwrap_or(next_position(conn, TABLE_TAGS)?);
    conn.execute(
        "INSERT OR REPLACE INTO tags (position, tag_id, label, json) VALUES (?1, ?2, ?3, ?4)",
        params![
            position,
            tag.tag_id.as_str(),
            tag.label.as_str(),
            to_json(tag)?
        ],
    )
    .map_err(|e| Td3Error::Other(format!("library: upsert sqlite tag '{}': {}", tag.label, e)))?;
    Ok(())
}

pub(in crate::library::persistence) fn write_item_tags(
    conn: &Connection,
    item_tags: &[(String, String)],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_ITEM_TAGS)?;
    let mut stmt = conn
        .prepare("INSERT INTO item_tags (position, item_id, tag_id) VALUES (?1, ?2, ?3)")
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite item_tags insert: {}", e)))?;
    for (idx, (item_id, tag_id)) in item_tags.iter().enumerate() {
        stmt.execute(params![idx as i64, item_id.as_str(), tag_id.as_str()])
            .map_err(|e| {
                Td3Error::Other(format!(
                    "library: insert sqlite item_tag '{}:{}': {}",
                    item_id, tag_id, e
                ))
            })?;
    }
    Ok(())
}

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

pub(in crate::library::persistence) fn write_pattern_analysis(
    conn: &Connection,
    analyses: &[PatternAnalysis],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_PATTERN_ANALYSIS)?;
    let mut stmt = conn
        .prepare("INSERT INTO pattern_analysis (position, item_id, json) VALUES (?1, ?2, ?3)")
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: prepare sqlite pattern_analysis insert: {}",
                e
            ))
        })?;
    for (idx, analysis) in analyses.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            analysis.item_id.as_str(),
            to_json(analysis)?
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite pattern_analysis '{}': {}",
                analysis.item_id, e
            ))
        })?;
    }
    Ok(())
}

pub(in crate::library::persistence) fn write_pattern_relations(
    conn: &Connection,
    relations: &[PatternRelation],
) -> Result<(), Td3Error> {
    clear_table(conn, TABLE_PATTERN_RELATIONS)?;
    let mut stmt = conn
        .prepare(
            "INSERT INTO pattern_relations (position, from_item_id, to_item_id, kind, json) VALUES (?1, ?2, ?3, ?4, ?5)",
        )
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite pattern_relations insert: {}", e)))?;
    for (idx, relation) in relations.iter().enumerate() {
        stmt.execute(params![
            idx as i64,
            relation.from_item_id.as_str(),
            relation.to_item_id.as_str(),
            relation_kind_text(relation.kind),
            to_json(relation)?,
        ])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: insert sqlite pattern_relation '{}->{}': {}",
                relation.from_item_id, relation.to_item_id, e
            ))
        })?;
    }
    Ok(())
}

fn relation_kind_text(kind: crate::library::model::RelationKind) -> &'static str {
    use crate::library::model::RelationKind;
    match kind {
        RelationKind::SameScale => "same_scale",
        RelationKind::SameRoot => "same_root",
        RelationKind::SameRhythm => "same_rhythm",
        RelationKind::NearDuplicate => "near_duplicate",
        RelationKind::AnalyzerRelated => "analyzer_related",
        RelationKind::ProgressionFamily => "progression_family",
    }
}

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

pub(in crate::library::persistence) fn to_json<T: Serialize>(
    value: &T,
) -> Result<String, Td3Error> {
    serde_json::to_string(value)
        .map_err(|e| Td3Error::Other(format!("library: serialize sqlite row: {}", e)))
}
