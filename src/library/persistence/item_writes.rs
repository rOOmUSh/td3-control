#![allow(dead_code)]

use super::transactions::*;
use super::*;

pub fn upsert_item(path: &Path, item: &LibraryItem) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite upsert item transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_item_row(&tx, item)?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite upsert item transaction: {}",
            e
        ))
    })
}

pub fn upsert_items(path: &Path, items: &[LibraryItem]) -> Result<(), Td3Error> {
    if items.is_empty() {
        return Ok(());
    }
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite upsert items transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    for item in items {
        upsert_item_row(&tx, item)?;
    }
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite upsert items transaction: {}",
            e
        ))
    })
}

pub fn delete_item_and_item_tags(path: &Path, item_id: &str) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite delete item+item_tags transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    tx.execute("DELETE FROM item_tags WHERE item_id = ?1", params![item_id])
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: delete sqlite item_tags '{}': {}",
                item_id, e
            ))
        })?;
    tx.execute("DELETE FROM items WHERE item_id = ?1", params![item_id])
        .map_err(|e| {
            Td3Error::Other(format!("library: delete sqlite item '{}': {}", item_id, e))
        })?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite delete item+item_tags transaction: {}",
            e
        ))
    })
}

pub fn upsert_tag(path: &Path, tag: &Tag) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite upsert tag transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_tag_row(&tx, tag)?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite upsert tag transaction: {}",
            e
        ))
    })
}

pub fn add_tag_to_item(path: &Path, item: &LibraryItem, tag_id: &str) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite add_tag_to_item transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_item_row(&tx, item)?;
    tx.execute(
        "INSERT OR IGNORE INTO item_tags (position, item_id, tag_id) VALUES (?1, ?2, ?3)",
        params![
            next_position(&tx, TABLE_ITEM_TAGS)?,
            item.item_id.as_str(),
            tag_id
        ],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: insert sqlite item_tag '{}:{}': {}",
            item.item_id, tag_id, e
        ))
    })?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite add_tag_to_item transaction: {}",
            e
        ))
    })
}

pub fn remove_tag_from_item(
    path: &Path,
    item: &LibraryItem,
    tag_id: Option<&str>,
) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite remove_tag_from_item transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    upsert_item_row(&tx, item)?;
    if let Some(tag_id) = tag_id {
        tx.execute(
            "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
            params![item.item_id.as_str(), tag_id],
        )
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: delete sqlite item_tag '{}:{}': {}",
                item.item_id, tag_id, e
            ))
        })?;
    }
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite remove_tag_from_item transaction: {}",
            e
        ))
    })
}

pub fn delete_item_tag_edge(
    path: &Path,
    item_id: &str,
    tag_id: Option<&str>,
) -> Result<(), Td3Error> {
    let Some(tag_id) = tag_id else {
        return Ok(());
    };
    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin sqlite delete_item_tag_edge transaction: {}",
            e
        ))
    })?;
    write_format_version(&tx)?;
    tx.execute(
        "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
        params![item_id, tag_id],
    )
    .map_err(|e| {
        Td3Error::Other(format!(
            "library: delete sqlite item_tag '{}:{}': {}",
            item_id, tag_id, e
        ))
    })?;
    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite delete_item_tag_edge transaction: {}",
            e
        ))
    })
}

pub fn save_items(path: &Path, items: &[LibraryItem]) -> Result<(), Td3Error> {
    let conn = open_partial_connection(path)?;
    let tx = conn
        .unchecked_transaction()
        .map_err(|e| Td3Error::Other(format!("library: begin sqlite items transaction: {}", e)))?;
    write_format_version(&tx)?;
    write_items(&tx, items)?;
    tx.commit()
        .map_err(|e| Td3Error::Other(format!("library: commit sqlite items transaction: {}", e)))
}

pub fn apply_bulk_tag(
    path: &Path,
    tags_to_upsert: &[Tag],
    items_to_upsert: &[LibraryItem],
    item_tags_to_add: &[(String, String)],
    item_tags_to_remove: &[(String, String)],
) -> Result<(), Td3Error> {
    if tags_to_upsert.is_empty()
        && items_to_upsert.is_empty()
        && item_tags_to_add.is_empty()
        && item_tags_to_remove.is_empty()
    {
        return Ok(());
    }

    let conn = open_partial_connection(path)?;
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!("library: begin sqlite bulk_tag transaction: {}", e))
    })?;
    write_format_version(&tx)?;

    for tag in tags_to_upsert {
        upsert_tag_row(&tx, tag)?;
    }
    for item in items_to_upsert {
        upsert_item_row(&tx, item)?;
    }
    for (item_id, tag_id) in item_tags_to_remove {
        tx.execute(
            "DELETE FROM item_tags WHERE item_id = ?1 AND tag_id = ?2",
            params![item_id.as_str(), tag_id.as_str()],
        )
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: bulk_tag delete sqlite item_tag '{}:{}': {}",
                item_id, tag_id, e
            ))
        })?;
    }
    for (item_id, tag_id) in item_tags_to_add {
        tx.execute(
            "INSERT OR IGNORE INTO item_tags (position, item_id, tag_id) VALUES (?1, ?2, ?3)",
            params![
                next_position(&tx, TABLE_ITEM_TAGS)?,
                item_id.as_str(),
                tag_id.as_str()
            ],
        )
        .map_err(|e| {
            Td3Error::Other(format!(
                "library: bulk_tag insert sqlite item_tag '{}:{}': {}",
                item_id, tag_id, e
            ))
        })?;
    }

    tx.commit().map_err(|e| {
        Td3Error::Other(format!(
            "library: commit sqlite bulk_tag transaction: {}",
            e
        ))
    })
}
