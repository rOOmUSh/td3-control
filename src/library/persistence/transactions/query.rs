use super::*;

pub(in crate::library::persistence) fn build_list_items_query(
    filter: &ItemFilter,
) -> (String, Vec<Value>) {
    let mut sql = String::from("SELECT json FROM items WHERE 1=1");
    let mut params: Vec<Value> = Vec::new();

    if let Some(q) = &filter.search {
        let trimmed = q.trim().to_lowercase();
        if !trimmed.is_empty() {
            let like = format!("%{}%", trimmed);
            sql.push_str(
                " AND (
                    lower(COALESCE(display_name, '')) LIKE ?
                    OR lower(COALESCE(source_label, '')) LIKE ?
                    OR lower(COALESCE(source_path, '')) LIKE ?
                    OR lower(COALESCE(notes, '')) LIKE ?
                    OR EXISTS (
                        SELECT 1
                        FROM item_tags it
                        JOIN tags t ON t.tag_id = it.tag_id
                        WHERE it.item_id = items.item_id
                          AND lower(t.label) LIKE ?
                    )
                )",
            );
            for _ in 0..5 {
                params.push(Value::Text(like.clone()));
            }
        }
    }

    if let Some(fmt) = &filter.format {
        sql.push_str(" AND format = ?");
        params.push(Value::Text(fmt.clone()));
    }
    if let Some(kind) = &filter.source_kind {
        sql.push_str(" AND source_kind = ?");
        params.push(Value::Text(source_kind_text(*kind)));
    }
    if let Some(favorite) = filter.favorite {
        sql.push_str(" AND favorite = ?");
        params.push(Value::Integer(if favorite { 1 } else { 0 }));
    }
    if let Some(archived) = filter.archived {
        sql.push_str(" AND archived = ?");
        params.push(Value::Integer(if archived { 1 } else { 0 }));
    }
    if filter.duplicate_only {
        sql.push_str(" AND duplicate_status IN ('exactduplicate', 'nearduplicate')");
    }
    if let Some(snapshot_id) = &filter.snapshot_id {
        sql.push_str(" AND snapshot_id = ?");
        params.push(Value::Text(snapshot_id.clone()));
    }
    if let Some(slot_key) = &filter.slot_key {
        sql.push_str(" AND slot_key = ?");
        params.push(Value::Text(slot_key.clone()));
    }
    if let Some(scale) = &filter.scale {
        sql.push_str(" AND scale_name = ?");
        params.push(Value::Text(scale.clone()));
    }
    if let Some(root) = &filter.root {
        sql.push_str(" AND root_note = ?");
        params.push(Value::Text(root.clone()));
    }
    if let Some(tag) = &filter.tag {
        sql.push_str(
            " AND EXISTS (
                SELECT 1
                FROM item_tags it
                JOIN tags t ON t.tag_id = it.tag_id
                WHERE it.item_id = items.item_id
                  AND t.label = ?
            )",
        );
        params.push(Value::Text(tag.clone()));
    }
    if let Some(from) = &filter.date_from {
        sql.push_str(" AND COALESCE(created_at, '') >= ?");
        params.push(Value::Text(from.clone()));
    }
    if let Some(to) = &filter.date_to {
        sql.push_str(" AND COALESCE(created_at, '') <= ?");
        params.push(Value::Text(to.clone()));
    }
    if filter.needs_review {
        sql.push_str(" AND analysis_status = 'needsreview'");
    }

    sql.push_str(" ORDER BY position");
    (sql, params)
}

pub(in crate::library::persistence) fn pad_snapshot_slots(
    snapshot_id: &str,
    stored: &[SnapshotSlot],
) -> Vec<SnapshotSlot> {
    let mut grid: Vec<SnapshotSlot> = Vec::with_capacity(64);
    for g in 1..=4u8 {
        for p in 1..=8u8 {
            for side in ['A', 'B'] {
                let key = format!("G{}-P{}{}", g, p, side);
                let existing = stored.iter().find(|s| s.slot_key == key).cloned();
                grid.push(existing.unwrap_or(SnapshotSlot {
                    snapshot_id: snapshot_id.to_string(),
                    slot_key: key,
                    item_id: None,
                    empty: true,
                    display_name: None,
                }));
            }
        }
    }
    grid
}
