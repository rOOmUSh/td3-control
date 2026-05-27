//! Forward-only schema migrations for the SQLite catalog.
//!
//! The catalog began as v1 with one normalized column per table key plus a
//! `json` payload that held everything else. v2 splits frequently-queried
//! `LibraryItem` fields out of that JSON payload into typed columns so the
//! filter SQL stops paying for `json_extract` per row, and introduces typed
//! indexes on those columns.
//!
//! Migration is idempotent: it inspects `metadata.format_version` and the
//! current `items` table shape, applies only the steps that are still
//! pending, and bumps the stored version to the current target on success.

use rusqlite::{params, Connection};

use crate::error::Td3Error;

use super::transactions::write_format_version;
use super::LibraryData;

/// All v2 columns added to the `items` table beyond the v1 base of
/// `(position, item_id, json)`. The list is the contract used by both the
/// forward migration and the column-population step. `created_at` and
/// `updated_at` are NULL-able even though `LibraryItem` requires them, so
/// migration of legacy rows that lack the field does not violate `NOT NULL`.
const V2_ITEM_COLUMNS: &[(&str, &str)] = &[
    ("display_name", "TEXT"),
    ("source_kind", "TEXT"),
    ("source_label", "TEXT"),
    ("source_path", "TEXT"),
    ("created_at", "TEXT"),
    ("updated_at", "TEXT"),
    ("favorite", "INTEGER NOT NULL DEFAULT 0"),
    ("archived", "INTEGER NOT NULL DEFAULT 0"),
    ("slot_key", "TEXT"),
    ("snapshot_id", "TEXT"),
    ("format", "TEXT"),
    ("scale_name", "TEXT"),
    ("root_note", "TEXT"),
    ("duplicate_status", "TEXT NOT NULL DEFAULT 'unknown'"),
    ("analysis_status", "TEXT NOT NULL DEFAULT 'unknown'"),
    ("notes", "TEXT"),
    ("content_hash", "TEXT"),
];

/// Names of the legacy json_extract-based indexes that v2 replaces with
/// typed-column indexes. Dropped during migration so v2 indexes can be
/// recreated on the same names.
const LEGACY_ITEM_INDEXES: &[&str] = &[
    "idx_items_content_hash",
    "idx_items_snapshot_id",
    "idx_items_format",
    "idx_items_source_kind",
    "idx_items_analysis_status",
];

/// Apply forward schema migrations on `conn`. Safe to call on every load,
/// including the very first one, because each step is idempotent.
pub(super) fn apply_schema_migrations(conn: &Connection) -> Result<(), Td3Error> {
    let stored = stored_format_version(conn)?;

    let Some(stored) = stored else {
        ensure_v2_item_columns(conn)?;
        ensure_v2_item_indexes(conn)?;
        return Ok(());
    };

    if stored == LibraryData::CURRENT_FORMAT_VERSION {
        ensure_v2_item_columns(conn)?;
        ensure_v2_item_indexes(conn)?;
        return Ok(());
    }
    if stored > LibraryData::CURRENT_FORMAT_VERSION {
        return Err(Td3Error::Other(format!(
            "library: sqlite catalog has unsupported format_version {} (this build supports up to {})",
            stored,
            LibraryData::CURRENT_FORMAT_VERSION
        )));
    }
    if stored < 1 {
        return Err(Td3Error::Other(format!(
            "library: sqlite catalog has invalid format_version {}",
            stored
        )));
    }

    if stored == 1 {
        migrate_v1_to_v2(conn)?;
    }

    Ok(())
}

fn migrate_v1_to_v2(conn: &Connection) -> Result<(), Td3Error> {
    let tx = conn.unchecked_transaction().map_err(|e| {
        Td3Error::Other(format!(
            "library: begin v1->v2 migration transaction: {}",
            e
        ))
    })?;

    ensure_v2_item_columns(&tx)?;
    populate_v2_item_columns_from_json(&tx)?;
    ensure_v2_item_indexes(&tx)?;
    write_format_version(&tx)?;

    tx.commit()
        .map_err(|e| Td3Error::Other(format!("library: commit v1->v2 migration: {}", e)))?;
    Ok(())
}

fn stored_format_version(conn: &Connection) -> Result<Option<u32>, Td3Error> {
    let raw: String = match conn.query_row(
        "SELECT value_text FROM metadata WHERE key = 'format_version'",
        [],
        |row| row.get::<_, String>(0),
    ) {
        Ok(raw) => raw,
        Err(rusqlite::Error::QueryReturnedNoRows) => return Ok(None),
        Err(e) => {
            return Err(Td3Error::Other(format!(
                "library: read stored format_version: {}",
                e
            )));
        }
    };
    let parsed = raw.parse::<u32>().map_err(|e| {
        Td3Error::Other(format!(
            "library: parse stored format_version '{}': {}",
            raw, e
        ))
    })?;
    Ok(Some(parsed))
}

fn ensure_v2_item_columns(conn: &Connection) -> Result<(), Td3Error> {
    let existing = item_table_columns(conn)?;
    for (name, decl) in V2_ITEM_COLUMNS {
        if !existing.iter().any(|c| c == name) {
            let sql = format!("ALTER TABLE items ADD COLUMN {} {}", name, decl);
            conn.execute(&sql, []).map_err(|e| {
                Td3Error::Other(format!("library: alter items add '{}': {}", name, e))
            })?;
        }
    }
    Ok(())
}

fn item_table_columns(conn: &Connection) -> Result<Vec<String>, Td3Error> {
    let mut stmt = conn
        .prepare("PRAGMA table_info(items)")
        .map_err(|e| Td3Error::Other(format!("library: prepare pragma items: {}", e)))?;
    let mut rows = stmt
        .query([])
        .map_err(|e| Td3Error::Other(format!("library: query pragma items: {}", e)))?;
    let mut out = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| Td3Error::Other(format!("library: iterate pragma items: {}", e)))?
    {
        let name: String = row
            .get(1)
            .map_err(|e| Td3Error::Other(format!("library: pragma items column name: {}", e)))?;
        out.push(name);
    }
    Ok(out)
}

fn populate_v2_item_columns_from_json(conn: &Connection) -> Result<(), Td3Error> {
    conn.execute(
        "UPDATE items SET
            display_name = json_extract(json, '$.display_name'),
            source_kind = json_extract(json, '$.source_kind'),
            source_label = json_extract(json, '$.source_label'),
            source_path = json_extract(json, '$.source_path'),
            created_at = json_extract(json, '$.created_at'),
            updated_at = json_extract(json, '$.updated_at'),
            favorite = COALESCE(json_extract(json, '$.favorite'), 0),
            archived = COALESCE(json_extract(json, '$.archived'), 0),
            slot_key = json_extract(json, '$.slot_key'),
            snapshot_id = json_extract(json, '$.snapshot_id'),
            format = json_extract(json, '$.format'),
            scale_name = json_extract(json, '$.scale_name'),
            root_note = json_extract(json, '$.root_note'),
            duplicate_status = COALESCE(json_extract(json, '$.duplicate_status'), 'unknown'),
            analysis_status = COALESCE(json_extract(json, '$.analysis_status'), 'unknown'),
            notes = json_extract(json, '$.notes'),
            content_hash = json_extract(json, '$.content_hash')",
        params![],
    )
    .map_err(|e| Td3Error::Other(format!("library: populate v2 item columns: {}", e)))?;
    Ok(())
}

fn ensure_v2_item_indexes(conn: &Connection) -> Result<(), Td3Error> {
    for name in LEGACY_ITEM_INDEXES {
        let sql = format!("DROP INDEX IF EXISTS {}", name);
        conn.execute(&sql, []).map_err(|e| {
            Td3Error::Other(format!("library: drop legacy index '{}': {}", name, e))
        })?;
    }

    conn.execute_batch(
        "
        CREATE INDEX IF NOT EXISTS idx_items_content_hash
            ON items (content_hash);

        CREATE INDEX IF NOT EXISTS idx_items_snapshot_id
            ON items (snapshot_id);

        CREATE INDEX IF NOT EXISTS idx_items_format
            ON items (format);

        CREATE INDEX IF NOT EXISTS idx_items_source_kind
            ON items (source_kind);

        CREATE INDEX IF NOT EXISTS idx_items_analysis_status
            ON items (analysis_status);

        CREATE INDEX IF NOT EXISTS idx_items_favorite
            ON items (favorite);

        CREATE INDEX IF NOT EXISTS idx_items_archived
            ON items (archived);

        CREATE INDEX IF NOT EXISTS idx_items_slot_key
            ON items (slot_key);

        CREATE INDEX IF NOT EXISTS idx_items_duplicate_status
            ON items (duplicate_status);
        ",
    )
    .map_err(|e| Td3Error::Other(format!("library: create v2 item indexes: {}", e)))?;
    Ok(())
}
