use super::*;

pub(in crate::library::persistence) fn init_schema(conn: &Connection) -> Result<(), Td3Error> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS metadata (
            key TEXT PRIMARY KEY,
            value_text TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS items (
            position INTEGER PRIMARY KEY,
            item_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL,
            display_name TEXT,
            source_kind TEXT,
            source_label TEXT,
            source_path TEXT,
            created_at TEXT,
            updated_at TEXT,
            favorite INTEGER NOT NULL DEFAULT 0,
            archived INTEGER NOT NULL DEFAULT 0,
            slot_key TEXT,
            snapshot_id TEXT,
            format TEXT,
            scale_name TEXT,
            root_note TEXT,
            duplicate_status TEXT NOT NULL DEFAULT 'unknown',
            analysis_status TEXT NOT NULL DEFAULT 'unknown',
            notes TEXT,
            content_hash TEXT
        );

        CREATE TABLE IF NOT EXISTS snapshots (
            position INTEGER PRIMARY KEY,
            snapshot_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS snapshot_slots (
            position INTEGER PRIMARY KEY,
            snapshot_id TEXT NOT NULL,
            slot_key TEXT NOT NULL,
            json TEXT NOT NULL,
            UNIQUE(snapshot_id, slot_key)
        );

        CREATE TABLE IF NOT EXISTS tags (
            position INTEGER PRIMARY KEY,
            tag_id TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS item_tags (
            position INTEGER PRIMARY KEY,
            item_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            UNIQUE(item_id, tag_id)
        );

        CREATE TABLE IF NOT EXISTS file_index (
            position INTEGER PRIMARY KEY,
            batch_id TEXT,
            path TEXT NOT NULL,
            json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS pattern_analysis (
            position INTEGER PRIMARY KEY,
            item_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS pattern_relations (
            position INTEGER PRIMARY KEY,
            from_item_id TEXT NOT NULL,
            to_item_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            json TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS import_batches (
            position INTEGER PRIMARY KEY,
            batch_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_item_tags_item
            ON item_tags (item_id);

        CREATE INDEX IF NOT EXISTS idx_tags_label
            ON tags (label);
        ",
    )
    .map_err(|e| Td3Error::Other(format!("library: init sqlite schema: {}", e)))?;
    Ok(())
}

pub(in crate::library::persistence) fn db_is_empty(conn: &Connection) -> Result<bool, Td3Error> {
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM metadata", [], |row| row.get(0))
        .map_err(|e| Td3Error::Other(format!("library: sqlite metadata count: {}", e)))?;
    Ok(count == 0)
}
