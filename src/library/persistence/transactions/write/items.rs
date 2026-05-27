use super::*;

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
