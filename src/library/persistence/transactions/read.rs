use super::*;

pub(in crate::library::persistence) fn load_data(
    conn: &Connection,
) -> Result<LibraryData, Td3Error> {
    let format_version = load_format_version(conn)?;
    if format_version != LibraryData::CURRENT_FORMAT_VERSION {
        return Err(Td3Error::Other(format!(
            "library: unsupported format_version {} in sqlite catalog (expected {})",
            format_version,
            LibraryData::CURRENT_FORMAT_VERSION
        )));
    }

    Ok(LibraryData {
        format_version,
        items: load_json_rows(
            conn,
            &format!("SELECT json FROM {} ORDER BY position", TABLE_ITEMS),
        )?,
        snapshots: load_json_rows(
            conn,
            &format!("SELECT json FROM {} ORDER BY position", TABLE_SNAPSHOTS),
        )?,
        snapshot_slots: load_json_rows(
            conn,
            &format!(
                "SELECT json FROM {} ORDER BY position",
                TABLE_SNAPSHOT_SLOTS
            ),
        )?,
        tags: load_json_rows(
            conn,
            &format!("SELECT json FROM {} ORDER BY position", TABLE_TAGS),
        )?,
        item_tags: load_item_tags(conn)?,
        file_index: load_json_rows(
            conn,
            &format!("SELECT json FROM {} ORDER BY position", TABLE_FILE_INDEX),
        )?,
        pattern_analysis: load_json_rows(
            conn,
            &format!(
                "SELECT json FROM {} ORDER BY position",
                TABLE_PATTERN_ANALYSIS
            ),
        )?,
        pattern_relations: load_json_rows(
            conn,
            &format!(
                "SELECT json FROM {} ORDER BY position",
                TABLE_PATTERN_RELATIONS
            ),
        )?,
        import_batches: load_json_rows(
            conn,
            &format!(
                "SELECT json FROM {} ORDER BY position",
                TABLE_IMPORT_BATCHES
            ),
        )?,
    })
}

pub(in crate::library::persistence) fn load_format_version(
    conn: &Connection,
) -> Result<u32, Td3Error> {
    let value = conn
        .query_row(
            "SELECT value_text FROM metadata WHERE key = 'format_version'",
            [],
            |row| row.get::<_, String>(0),
        )
        .map_err(|e| Td3Error::Other(format!("library: read sqlite format_version: {}", e)))?;
    value.parse::<u32>().map_err(|e| {
        Td3Error::Other(format!(
            "library: parse sqlite format_version '{}': {}",
            value, e
        ))
    })
}

pub(in crate::library::persistence) fn load_json_rows<T>(
    conn: &Connection,
    sql: &str,
) -> Result<Vec<T>, Td3Error>
where
    T: DeserializeOwned,
{
    load_json_rows_with_params(conn, sql, Vec::new())
}

pub(in crate::library::persistence) fn load_json_rows_with_params<T>(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<T>, Td3Error>
where
    T: DeserializeOwned,
{
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite load '{}': {}", sql, e)))?;
    let mut rows = stmt
        .query(params_from_iter(params.iter()))
        .map_err(|e| Td3Error::Other(format!("library: query sqlite load '{}': {}", sql, e)))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| Td3Error::Other(format!("library: iterate sqlite rows '{}': {}", sql, e)))?
    {
        let json: String = row
            .get(0)
            .map_err(|e| Td3Error::Other(format!("library: sqlite row decode '{}': {}", sql, e)))?;
        let decoded = serde_json::from_str::<T>(&json).map_err(|e| {
            Td3Error::Other(format!("library: deserialize sqlite row '{}': {}", sql, e))
        })?;
        out.push(decoded);
    }
    Ok(out)
}

pub(in crate::library::persistence) fn load_one_json_row<T>(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Option<T>, Td3Error>
where
    T: DeserializeOwned,
{
    let mut rows = load_json_rows_with_params::<T>(conn, sql, params)?;
    Ok(rows.pop())
}

pub(in crate::library::persistence) fn load_text_rows_with_params(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<String>, Td3Error> {
    let mut stmt = conn.prepare(sql).map_err(|e| {
        Td3Error::Other(format!(
            "library: prepare sqlite text load '{}': {}",
            sql, e
        ))
    })?;
    let mut rows = stmt.query(params_from_iter(params.iter())).map_err(|e| {
        Td3Error::Other(format!("library: query sqlite text load '{}': {}", sql, e))
    })?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| {
        Td3Error::Other(format!(
            "library: iterate sqlite text rows '{}': {}",
            sql, e
        ))
    })? {
        let value: String = row.get(0).map_err(|e| {
            Td3Error::Other(format!("library: sqlite text row decode '{}': {}", sql, e))
        })?;
        out.push(value);
    }
    Ok(out)
}

pub(in crate::library::persistence) fn load_positioned_json_rows_with_params<T>(
    conn: &Connection,
    sql: &str,
    params: Vec<Value>,
) -> Result<Vec<(i64, T)>, Td3Error>
where
    T: DeserializeOwned,
{
    let mut stmt = conn.prepare(sql).map_err(|e| {
        Td3Error::Other(format!(
            "library: prepare sqlite positioned load '{}': {}",
            sql, e
        ))
    })?;
    let mut rows = stmt.query(params_from_iter(params.iter())).map_err(|e| {
        Td3Error::Other(format!(
            "library: query sqlite positioned load '{}': {}",
            sql, e
        ))
    })?;
    let mut out = Vec::new();
    while let Some(row) = rows.next().map_err(|e| {
        Td3Error::Other(format!(
            "library: iterate sqlite positioned rows '{}': {}",
            sql, e
        ))
    })? {
        let position: i64 = row.get(0).map_err(|e| {
            Td3Error::Other(format!(
                "library: sqlite positioned row decode '{}': {}",
                sql, e
            ))
        })?;
        let json: String = row.get(1).map_err(|e| {
            Td3Error::Other(format!(
                "library: sqlite positioned json decode '{}': {}",
                sql, e
            ))
        })?;
        let decoded = serde_json::from_str::<T>(&json).map_err(|e| {
            Td3Error::Other(format!(
                "library: deserialize sqlite positioned row '{}': {}",
                sql, e
            ))
        })?;
        out.push((position, decoded));
    }
    Ok(out)
}

pub(in crate::library::persistence) fn load_item_tags(
    conn: &Connection,
) -> Result<Vec<(String, String)>, Td3Error> {
    let mut stmt = conn
        .prepare("SELECT item_id, tag_id FROM item_tags ORDER BY position")
        .map_err(|e| Td3Error::Other(format!("library: prepare sqlite item_tags: {}", e)))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| Td3Error::Other(format!("library: query sqlite item_tags: {}", e)))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| Td3Error::Other(format!("library: iterate sqlite item_tags: {}", e)))?
    {
        let item_id: String = row
            .get(0)
            .map_err(|e| Td3Error::Other(format!("library: sqlite item_tags item_id: {}", e)))?;
        let tag_id: String = row
            .get(1)
            .map_err(|e| Td3Error::Other(format!("library: sqlite item_tags tag_id: {}", e)))?;
        out.push((item_id, tag_id));
    }
    Ok(out)
}
