use serde::Serialize;

use super::*;

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

pub(in crate::library::persistence) fn to_json<T: Serialize>(
    value: &T,
) -> Result<String, Td3Error> {
    serde_json::to_string(value)
        .map_err(|e| Td3Error::Other(format!("library: serialize sqlite row: {}", e)))
}
