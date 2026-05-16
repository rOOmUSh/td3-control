//! Tests for the v1 -> v2 SQLite schema migration in the library catalog.
//!
//! v1 stored every queryable `LibraryItem` field inside a single `json` blob
//! and indexed them with `json_extract` expression indexes. v2 lifts the
//! filterable fields into typed columns and replaces the json_extract
//! indexes. These tests pin the migration contract so v1 user catalogs
//! continue to load and so newly written rows populate the typed columns.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::{params, Connection};

use crate::library::model::{
    AnalysisStatus, DuplicateStatus, LibraryItem, SourceKind,
};
use crate::library::store::{self, LibraryStore};
use crate::library::ItemFilter;

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_dir(tag: &str) -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!(
        "td3-library-migration-{}-{}-{}-{}",
        tag, pid, n, nanos
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp library dir");
    dir
}

fn sample_item(id: &str, name: &str, hash: &str) -> LibraryItem {
    LibraryItem {
        item_id: id.to_string(),
        display_name: name.to_string(),
        source_kind: SourceKind::File,
        source_label: "sample".to_string(),
        source_path: Some(format!("/tmp/{}.seq", name)),
        created_at: "20260101T000000Z".to_string(),
        updated_at: "20260102T000000Z".to_string(),
        tags: vec![],
        favorite: true,
        archived: false,
        slot_key: Some("G1-P1A".to_string()),
        snapshot_id: Some("snap_1".to_string()),
        snapshot_name: Some("snap-name".to_string()),
        format: Some("seq".to_string()),
        scale_name: Some("minor".to_string()),
        root_note: Some("A".to_string()),
        duplicate_status: DuplicateStatus::ExactDuplicate,
        related_group_count: 0,
        analysis_status: AnalysisStatus::NeedsReview,
        notes: Some("a note".to_string()),
        content_hash: Some(hash.to_string()),
    }
}

/// Build a v1-shaped SQLite catalog at `path` with a single item row stored
/// as a JSON blob, and v1's `json_extract`-based index on `content_hash`.
/// `format_version` is set to `1` so the migration step actually fires.
fn make_v1_catalog(path: &std::path::Path, item: &LibraryItem) {
    let conn = Connection::open(path).expect("open sqlite");
    conn.execute_batch(
        "
        CREATE TABLE metadata (
            key TEXT PRIMARY KEY,
            value_text TEXT NOT NULL
        );
        CREATE TABLE items (
            position INTEGER PRIMARY KEY,
            item_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );
        CREATE TABLE snapshots (
            position INTEGER PRIMARY KEY,
            snapshot_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );
        CREATE TABLE snapshot_slots (
            position INTEGER PRIMARY KEY,
            snapshot_id TEXT NOT NULL,
            slot_key TEXT NOT NULL,
            json TEXT NOT NULL,
            UNIQUE(snapshot_id, slot_key)
        );
        CREATE TABLE tags (
            position INTEGER PRIMARY KEY,
            tag_id TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE item_tags (
            position INTEGER PRIMARY KEY,
            item_id TEXT NOT NULL,
            tag_id TEXT NOT NULL,
            UNIQUE(item_id, tag_id)
        );
        CREATE TABLE file_index (
            position INTEGER PRIMARY KEY,
            batch_id TEXT,
            path TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE pattern_analysis (
            position INTEGER PRIMARY KEY,
            item_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );
        CREATE TABLE pattern_relations (
            position INTEGER PRIMARY KEY,
            from_item_id TEXT NOT NULL,
            to_item_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            json TEXT NOT NULL
        );
        CREATE TABLE import_batches (
            position INTEGER PRIMARY KEY,
            batch_id TEXT NOT NULL UNIQUE,
            json TEXT NOT NULL
        );
        CREATE INDEX idx_items_content_hash
            ON items (json_extract(json, '$.content_hash'));
        CREATE INDEX idx_items_format
            ON items (json_extract(json, '$.format'));
        ",
    )
    .expect("create v1 schema");

    conn.execute(
        "INSERT INTO metadata (key, value_text) VALUES ('format_version', '1')",
        [],
    )
    .expect("write v1 format_version");

    let json = serde_json::to_string(item).expect("serialize item");
    conn.execute(
        "INSERT INTO items (position, item_id, json) VALUES (?1, ?2, ?3)",
        params![0_i64, item.item_id.as_str(), json.as_str()],
    )
    .expect("insert v1 item");
}

fn read_format_version(path: &std::path::Path) -> u32 {
    let conn = Connection::open(path).expect("open sqlite for read");
    let raw: String = conn
        .query_row(
            "SELECT value_text FROM metadata WHERE key = 'format_version'",
            [],
            |row| row.get(0),
        )
        .expect("read format_version");
    raw.parse::<u32>().expect("parse format_version")
}

fn pragma_columns(path: &std::path::Path, table: &str) -> Vec<String> {
    let conn = Connection::open(path).expect("open sqlite for pragma");
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", table))
        .expect("prepare pragma");
    let mut rows = stmt.query([]).expect("query pragma");
    let mut out = Vec::new();
    while let Some(row) = rows.next().expect("iterate pragma") {
        let name: String = row.get(1).expect("decode pragma name");
        out.push(name);
    }
    out
}

fn list_indexes(path: &std::path::Path, table: &str) -> Vec<(String, String)> {
    let conn = Connection::open(path).expect("open sqlite for index list");
    let mut stmt = conn
        .prepare("SELECT name, COALESCE(sql, '') FROM sqlite_master WHERE type = 'index' AND tbl_name = ?1")
        .expect("prepare index list");
    let mut rows = stmt.query(params![table]).expect("query index list");
    let mut out = Vec::new();
    while let Some(row) = rows.next().expect("iterate index list") {
        let name: String = row.get(0).expect("decode index name");
        let sql: String = row.get(1).expect("decode index sql");
        out.push((name, sql));
    }
    out
}

fn read_text_column(path: &std::path::Path, item_id: &str, column: &str) -> Option<String> {
    let conn = Connection::open(path).expect("open sqlite for column read");
    let sql = format!("SELECT {} FROM items WHERE item_id = ?1", column);
    conn.query_row(&sql, params![item_id], |row| row.get::<_, Option<String>>(0))
        .expect("read text column")
}

fn read_int_column(path: &std::path::Path, item_id: &str, column: &str) -> i64 {
    let conn = Connection::open(path).expect("open sqlite for int column");
    let sql = format!("SELECT {} FROM items WHERE item_id = ?1", column);
    conn.query_row(&sql, params![item_id], |row| row.get::<_, i64>(0))
        .expect("read int column")
}

fn read_required_text_column(path: &std::path::Path, item_id: &str, column: &str) -> String {
    read_text_column(path, item_id, column).unwrap_or_else(|| {
        panic!("expected non-null {} for {}", column, item_id);
    })
}

#[test]
fn migration_v1_populates_typed_columns() {
    let dir = temp_dir("populate-cols");
    let path = dir.join("catalog.sqlite3");

    let item = sample_item("item_a", "alpha", "deadbeef");
    make_v1_catalog(&path, &item);

    let store = LibraryStore::load_or_create(&path).expect("load triggers migration");

    let columns = pragma_columns(&path, "items");
    for required in &[
        "format",
        "source_kind",
        "favorite",
        "archived",
        "duplicate_status",
        "snapshot_id",
        "slot_key",
        "scale_name",
        "root_note",
        "analysis_status",
        "created_at",
        "updated_at",
        "content_hash",
        "display_name",
        "source_label",
        "source_path",
        "notes",
    ] {
        assert!(
            columns.iter().any(|c| c == *required),
            "items table is missing column {} after migration. columns: {:?}",
            required,
            columns
        );
    }

    assert_eq!(
        read_text_column(&path, "item_a", "format").as_deref(),
        Some("seq")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "source_kind").as_deref(),
        Some("file")
    );
    assert_eq!(read_int_column(&path, "item_a", "favorite"), 1);
    assert_eq!(read_int_column(&path, "item_a", "archived"), 0);
    assert_eq!(
        read_text_column(&path, "item_a", "snapshot_id").as_deref(),
        Some("snap_1")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "slot_key").as_deref(),
        Some("G1-P1A")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "scale_name").as_deref(),
        Some("minor")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "root_note").as_deref(),
        Some("A")
    );
    assert_eq!(
        read_required_text_column(&path, "item_a", "duplicate_status"),
        "exactduplicate"
    );
    assert_eq!(
        read_required_text_column(&path, "item_a", "analysis_status"),
        "needsreview"
    );
    assert_eq!(
        read_text_column(&path, "item_a", "created_at").as_deref(),
        Some("20260101T000000Z")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "content_hash").as_deref(),
        Some("deadbeef")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "display_name").as_deref(),
        Some("alpha")
    );
    assert_eq!(
        read_text_column(&path, "item_a", "source_path").as_deref(),
        Some("/tmp/alpha.seq")
    );

    drop(store);

    assert_eq!(
        read_format_version(&path),
        store::LibraryData::CURRENT_FORMAT_VERSION
    );
}

#[test]
fn migration_v1_preserves_existing_items() {
    let dir = temp_dir("preserves");
    let path = dir.join("catalog.sqlite3");

    let item = sample_item("item_b", "beta", "cafef00d");
    make_v1_catalog(&path, &item);

    let store = LibraryStore::load_or_create(&path).expect("load triggers migration");
    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1, "v1 item should still load after migration");
    assert_eq!(items[0].item_id, "item_b");
    assert_eq!(items[0].display_name, "beta");
    assert_eq!(items[0].duplicate_status, DuplicateStatus::ExactDuplicate);
    assert_eq!(items[0].analysis_status, AnalysisStatus::NeedsReview);
    assert_eq!(items[0].content_hash.as_deref(), Some("cafef00d"));
}

#[test]
fn migration_v1_rebuilds_typed_indexes() {
    let dir = temp_dir("rebuilds-idx");
    let path = dir.join("catalog.sqlite3");

    let item = sample_item("item_c", "gamma", "abc12345");
    make_v1_catalog(&path, &item);

    let _store = LibraryStore::load_or_create(&path).expect("load triggers migration");

    let indexes = list_indexes(&path, "items");
    let by_name: std::collections::HashMap<String, String> = indexes.into_iter().collect();

    for required in &[
        "idx_items_content_hash",
        "idx_items_snapshot_id",
        "idx_items_format",
        "idx_items_source_kind",
        "idx_items_analysis_status",
    ] {
        let sql = by_name
            .get(*required)
            .unwrap_or_else(|| panic!("missing index {} after migration", required));
        assert!(
            !sql.contains("json_extract"),
            "index {} should not reference json_extract after migration. sql: {}",
            required,
            sql
        );
    }
}

#[test]
fn migration_is_idempotent_on_repeated_open() {
    let dir = temp_dir("idempotent");
    let path = dir.join("catalog.sqlite3");

    let item = sample_item("item_d", "delta", "11112222");
    make_v1_catalog(&path, &item);

    let _ = LibraryStore::load_or_create(&path).expect("first open migrates");
    let store = LibraryStore::load_or_create(&path).expect("second open is no-op migrate");

    let items = store.list_items(&ItemFilter::default()).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].item_id, "item_d");

    assert_eq!(
        read_format_version(&path),
        store::LibraryData::CURRENT_FORMAT_VERSION
    );
}

#[test]
fn migration_rejects_future_format_version() {
    let dir = temp_dir("future");
    let path = dir.join("catalog.sqlite3");

    {
        let conn = Connection::open(&path).expect("open sqlite");
        conn.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value_text TEXT NOT NULL);",
        )
        .expect("create metadata");
        conn.execute(
            "INSERT INTO metadata (key, value_text) VALUES ('format_version', '999')",
            [],
        )
        .expect("write impossible version");
    }

    let result = LibraryStore::load_or_create(&path);
    assert!(
        result.is_err(),
        "loading a catalog newer than this build must fail loudly, not silently downgrade"
    );
}

#[test]
fn migration_rejects_unreadable_format_version_metadata() {
    let dir = temp_dir("bad-version-type");
    let path = dir.join("catalog.sqlite3");

    {
        let conn = Connection::open(&path).expect("open sqlite");
        conn.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value_text BLOB NOT NULL);",
        )
        .expect("create metadata");
        conn.execute(
            "INSERT INTO metadata (key, value_text) VALUES ('format_version', x'ff')",
            [],
        )
        .expect("write blob format version");
    }

    let err = match LibraryStore::load_or_create(&path) {
        Ok(_) => panic!("invalid metadata must fail"),
        Err(err) => err,
    };
    let message = err.to_string();
    assert!(
        message.contains("read stored format_version"),
        "unexpected migration error: {}",
        message
    );
}

#[test]
fn fresh_catalog_writes_typed_columns_directly() {
    let dir = temp_dir("fresh-cols");
    let path = dir.join("catalog.sqlite3");

    let store = LibraryStore::load_or_create(&path).expect("create fresh store");
    let item = sample_item("item_e", "epsilon", "55556666");
    store.upsert_item(item.clone()).expect("upsert item");

    assert_eq!(
        read_text_column(&path, "item_e", "format").as_deref(),
        Some("seq")
    );
    assert_eq!(
        read_text_column(&path, "item_e", "content_hash").as_deref(),
        Some("55556666")
    );
    assert_eq!(read_int_column(&path, "item_e", "favorite"), 1);
    assert_eq!(
        read_required_text_column(&path, "item_e", "duplicate_status"),
        "exactduplicate"
    );
    assert_eq!(
        read_required_text_column(&path, "item_e", "analysis_status"),
        "needsreview"
    );
}

#[test]
fn list_items_filter_uses_typed_columns() {
    let dir = temp_dir("filter-cols");
    let path = dir.join("catalog.sqlite3");

    let store = LibraryStore::load_or_create(&path).expect("create fresh store");
    let mut a = sample_item("item_f", "fox", "aaaaaaaa");
    a.format = Some("seq".to_string());
    a.scale_name = Some("minor".to_string());
    let mut b = sample_item("item_g", "goat", "bbbbbbbb");
    b.format = Some("syx".to_string());
    b.scale_name = Some("major".to_string());
    b.favorite = false;
    b.duplicate_status = DuplicateStatus::Unique;
    b.analysis_status = AnalysisStatus::Ready;

    store.upsert_item(a).expect("upsert a");
    store.upsert_item(b).expect("upsert b");

    let only_seq = store
        .list_items(&ItemFilter {
            format: Some("seq".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_seq.len(), 1);
    assert_eq!(only_seq[0].item_id, "item_f");

    let only_major = store
        .list_items(&ItemFilter {
            scale: Some("major".to_string()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_major.len(), 1);
    assert_eq!(only_major[0].item_id, "item_g");

    let only_favorites = store
        .list_items(&ItemFilter {
            favorite: Some(true),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_favorites.len(), 1);
    assert_eq!(only_favorites[0].item_id, "item_f");

    let only_dups = store
        .list_items(&ItemFilter {
            duplicate_only: true,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(only_dups.len(), 1);
    assert_eq!(only_dups[0].item_id, "item_f");
}
