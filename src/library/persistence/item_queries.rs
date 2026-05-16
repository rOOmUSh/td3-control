use super::transactions::*;
use super::*;

pub fn list_items(path: &Path, filter: &ItemFilter) -> Result<Vec<LibraryItem>, Td3Error> {
    let conn = open_partial_connection(path)?;
    let (sql, params) = build_list_items_query(filter);
    load_json_rows_with_params(&conn, &sql, params)
}

pub fn get_item(path: &Path, id: &str) -> Result<Option<LibraryItem>, Td3Error> {
    if id.is_empty() {
        return Ok(None);
    }
    let conn = open_partial_connection(path)?;
    load_one_json_row(
        &conn,
        "SELECT json FROM items WHERE item_id = ?1 LIMIT 1",
        vec![Value::Text(id.to_string())],
    )
}

pub fn find_item_by_content_hash(path: &Path, hash: &str) -> Result<Option<LibraryItem>, Td3Error> {
    if hash.is_empty() {
        return Ok(None);
    }
    let conn = open_partial_connection(path)?;
    load_one_json_row(
        &conn,
        "SELECT json FROM items WHERE content_hash = ?1 ORDER BY position LIMIT 1",
        vec![Value::Text(hash.to_string())],
    )
}

pub fn list_tags(path: &Path) -> Result<Vec<Tag>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows(&conn, "SELECT json FROM tags ORDER BY position")
}

pub fn get_tag_by_label(path: &Path, label: &str) -> Result<Option<Tag>, Td3Error> {
    if label.is_empty() {
        return Ok(None);
    }
    let conn = open_partial_connection(path)?;
    load_one_json_row(
        &conn,
        "SELECT json FROM tags WHERE label = ?1 ORDER BY position LIMIT 1",
        vec![Value::Text(label.to_string())],
    )
}

pub fn list_file_index(path: &Path) -> Result<Vec<FileIndexEntry>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows(&conn, "SELECT json FROM file_index ORDER BY position")
}

pub fn list_batch_entries(path: &Path, batch_id: &str) -> Result<Vec<FileIndexEntry>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows_with_params(
        &conn,
        "SELECT json FROM file_index WHERE batch_id = ?1 ORDER BY position",
        vec![Value::Text(batch_id.to_string())],
    )
}

pub fn list_failed_entries(path: &Path) -> Result<Vec<FileIndexEntry>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows_with_params(
        &conn,
        "SELECT json FROM file_index WHERE json_extract(json, '$.status') = 'failed' ORDER BY position",
        Vec::new(),
    )
}

pub fn list_import_batches(path: &Path) -> Result<Vec<ImportBatch>, Td3Error> {
    let conn = open_partial_connection(path)?;
    load_json_rows(&conn, "SELECT json FROM import_batches ORDER BY position")
}

pub fn get_import_batch(path: &Path, id: &str) -> Result<Option<ImportBatch>, Td3Error> {
    if id.is_empty() {
        return Ok(None);
    }
    let conn = open_partial_connection(path)?;
    load_one_json_row(
        &conn,
        "SELECT json FROM import_batches WHERE batch_id = ?1 LIMIT 1",
        vec![Value::Text(id.to_string())],
    )
}
