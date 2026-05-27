use super::*;

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
